# Hiring Agent Zombie — Agent-Executable Integration Guide

**Published at:** docs.usezombie.com/integrations/hiring-agent
**Zombie type:** Hiring Agent
**Integration actors:** Slack (input trigger) → Zombie (reasoning) → Slack thread (output) + ATS (optional)
**Human involvement:** One-time grant approval. Offer letters and rejections require per-action approval.

---

## What this zombie does

The Hiring Agent Zombie watches a designated Slack channel (e.g. `#hiring`). When an HR
team member posts a candidate email, resume snippet, or ping, the zombie:
- Reads the candidate context
- Drafts a structured response (interview questions, scheduling message, feedback summary)
- Posts the response back to the same Slack thread
- Routes decisions (schedule, reject, advance) to the ATS if configured
- Requires human approval for irreversible actions (send offer, send rejection)

---

## Actors

```
[HR team posts in #hiring]
    → Slack Events API
        → UseZombie (webhook trigger)
            → Hiring Agent Zombie (reasoning: parse candidate context, draft response)
                → Slack API (post to thread)     [UseZombie injects Slack token]
                → Lever / Greenhouse (optional)  [UseZombie injects ATS key]
                → Human (approval gate)          [for offer/rejection actions]
```

---

## Prerequisites

| Credential ref | What it is | How to store |
|----------------|------------|-------------|
| `slack` | Slack bot token (must have `channels:read`, `chat:write`, `reactions:write`) | `zombiectl credential add slack` |
| `lever` | Lever API key (optional ATS) | `zombiectl credential add lever` |
| `greenhouse` | Greenhouse API key (optional ATS) | `zombiectl credential add greenhouse` |

---

## Setup — step by step

### Step 1: Create the zombie

```bash
zombiectl zombie create \
  --name "Hiring Agent" \
  --description "Watches #hiring channel, drafts interview responses, routes to ATS"
```

Note the `zombie_id`.

### Step 2: Configure the Slack Events API trigger

```bash
# Get the webhook URL
zombiectl zombie triggers list --zombie {zombie_id}
# Returns: https://api.usezombie.com/v1/webhooks/{zombie_id}
```

In the Slack API dashboard for your app:
1. Enable **Event Subscriptions**
2. Set Request URL: `https://api.usezombie.com/v1/webhooks/{zombie_id}`
3. Subscribe to bot events: `message.channels`
4. Add the bot to `#hiring` channel: `/invite @hiring-agent`

### Step 3: Request integration grants

```bash
# Slack grant — for reading events and posting responses
curl -X POST https://api.usezombie.com/v1/workspaces/{workspace_id}/zombies/{zombie_id}/integration-requests \
  -H "Authorization: Bearer {your_workspace_token}" \
  -d '{
    "service": "slack",
    "reason": "Need to read messages in #hiring and post responses to candidate threads"
  }'

# ATS grant (if using Lever)
curl -X POST https://api.usezombie.com/v1/workspaces/{workspace_id}/zombies/{zombie_id}/integration-requests \
  -H "Authorization: Bearer {your_workspace_token}" \
  -d '{
    "service": "lever",
    "reason": "Need to create and update candidate records based on #hiring thread outcomes"
  }'
```

Approve the grants via Slack DM, Discord, or dashboard.

### Step 4: Configure the approval gate for irreversible actions

Sending offers and rejections require human approval — configure this in the zombie's
firewall rules:

```bash
zombiectl zombie firewall set --zombie {zombie_id} --config '{
  "endpoint_rules": [
    {
      "method": "POST",
      "domain": "slack.com",
      "path": "/api/chat.postMessage",
      "action": "requires_approval",
      "condition": "body.text contains offer OR body.text contains rejection"
    },
    {
      "method": "POST",
      "domain": "hire.lever.co",
      "path": "/v1/offers",
      "action": "requires_approval"
    }
  ]
}'
```

---

## Runtime — what the zombie does on each Slack message

When someone posts in `#hiring`, the zombie receives the Slack event payload and:

1. **Parses the message** to detect candidate signals:
   - Resume or LinkedIn URL pasted
   - Candidate name + role mentioned
   - "Can we schedule?" or "What's the status?" type queries

2. **Drafts a response** (reasoning step — no external call unless context retrieval needed)

3. **Posts to the Slack thread:**
   ```
   POST /v1/execute
   {
     target: "slack.com/api/chat.postMessage",
     credential_ref: "slack",
     body: {
       channel: "C0HIRING123",
       thread_ts: "{original_message_ts}",
       text: "Hi team — I've reviewed Jane's profile. Recommended next step: technical screen.\n\nDraft invite:\n..."
     }
   }
   ```
   UseZombie checks Slack grant → injects bot token → posts to thread.

4. **For scheduling actions** — updates ATS record:
   ```
   POST /v1/execute
   {
     target: "hire.lever.co/v1/opportunities/{opp_id}/notes",
     credential_ref: "lever",
     body: {value: "Technical screen scheduled for {date}"}
   }
   ```

5. **For offer/rejection** — approval gate fires:
   ```
   UseZombie → Slack DM to workspace owner:
   "Hiring Agent wants to send an offer letter to Jane Smith (Senior Engineer) — Approve?"
   [Approve] [Deny]
   ```
   Human approves → UseZombie resumes execution → offer sent via Lever API.

---

## Example: Full thread walkthrough

```
HR posts in #hiring:
  "Jane Smith (jane@acme.com) applied for Senior Engineer.
   Resume: [link]. LI: [link]. Thoughts?"

Zombie response in thread:
  "Reviewed Jane's profile. Strong systems background, Zig/Rust experience.
   Recommend: technical screen.

   Draft calendar invite:
   Subject: UseZombie — Senior Engineer Technical Interview
   Time: [propose 3 slots]
   Interviewer: [suggest based on team]

   ATS updated: Stage → Technical Screen"

HR replies: "Looks good, send the invite"

Zombie:
  [Approval gate fires — posting calendar invite is a gated action]
  → Slack DM to owner: "Hiring Agent wants to send interview invite to Jane Smith — Approve?"
  → Owner approves
  → Invite sent
```

---

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| Zombie not responding to `#hiring` | Slack events not reaching webhook | Check Slack API dashboard → Event Subscriptions → verify URL |
| `UZ-GRANT-001` | No Slack grant approved | Request grant, approve via Slack DM |
| Response posted to wrong channel | Zombie using channel_id not channel name | Ensure event payload `channel` field passed correctly |
| ATS update failing | Lever/Greenhouse credential missing | `zombiectl credential add lever` |
| Approval gate not firing for offers | Firewall rule not matching | Check rule condition string matches your message pattern |

---

## External agent variant (Path B)

If your hiring pipeline runs in a separate system (e.g. a CrewAI crew processing
candidate data from your ATS sync), use an external agent key:

```bash
zombiectl agent create --workspace {ws} --name "crewai-hiring-pipeline"
# Save the zmb_ key

# In your CrewAI tool:
import requests
result = requests.post(
    "https://api.usezombie.com/v1/execute",
    headers={"Authorization": "Bearer zmb_your_key_here"},
    json={
        "zombie_id": "{zombie_id}",
        "target": "slack.com/api/chat.postMessage",
        "method": "POST",
        "credential_ref": "slack",
        "body": {"channel": "C0HIRING123", "thread_ts": ts, "text": draft}
    }
)
```
