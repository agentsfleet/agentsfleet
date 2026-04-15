# Lead Collector Zombie — Agent-Executable Integration Guide

**Published at:** docs.usezombie.com/integrations/lead-collector
**Zombie type:** Lead Collector
**Integration actors:** AgentMail or Gmail (input) → Zombie (reasoning) → Slack + CRM (output)
**Human involvement:** One-time grant approval per service. High-value lead actions only.

---

## What this zombie does

The Lead Collector Zombie monitors an inbox (AgentMail or Gmail), extracts structured
lead data from incoming emails, scores the lead, and takes action:
- Low score → files to CRM silently
- High score → posts to Slack for human review
- Hot lead (e.g. enterprise inbound) → triggers human-in-loop approval before acting

The human approves the zombie's integration access once during setup. After that, the
zombie operates autonomously within those approved bounds.

---

## Actors

```
[Sender email]
    → AgentMail / Gmail inbox
        → UseZombie (webhook trigger)
            → Lead Collector Zombie (reasoning: extract, score, route)
                → CRM API (persist lead)     [UseZombie injects CRM key]
                → Slack API (notify team)    [UseZombie injects Slack token]
                → Human (approval gate)      [for hot leads only]
```

---

## Prerequisites

Before the zombie can run, the workspace must have these credentials stored in the
UseZombie vault:

| Credential ref | What it is | How to store |
|----------------|------------|-------------|
| `agentmail` | AgentMail API key | `zombiectl credential add agentmail` |
| `gmail` | Gmail OAuth token | `zombiectl credential add gmail` |
| `slack` | Slack bot token | `zombiectl credential add slack` |
| `hubspot` | HubSpot API key (or your CRM) | `zombiectl credential add hubspot` |

Store exactly one of `agentmail` or `gmail` depending on your inbox provider.

---

## Setup — step by step

### Step 1: Create the zombie

```bash
zombiectl zombie create \
  --name "Lead Collector" \
  --description "Monitors inbox, extracts and scores leads, routes to CRM and Slack"
```

Note the `zombie_id` returned — you will use it in all subsequent steps.

### Step 2: Configure the trigger (AgentMail)

```bash
# Get your AgentMail webhook URL from UseZombie
zombiectl zombie triggers list --zombie {zombie_id}

# Configure AgentMail to forward to this webhook:
# agentmail.to dashboard → Inbox → Webhooks → Add
# URL: https://api.usezombie.com/v1/webhooks/{zombie_id}
# Events: message.received
```

Or for Gmail:
```bash
# Use Gmail Push Notifications (Pub/Sub)
# UseZombie endpoint: https://api.usezombie.com/v1/webhooks/{zombie_id}
```

### Step 3: Request integration grants

The zombie requests access to the services it needs. Run this once — UseZombie notifies
you for approval and the zombie remembers the approved grants.

```bash
# Request Slack grant
curl -X POST https://api.usezombie.com/v1/workspaces/{workspace_id}/zombies/{zombie_id}/integration-requests \
  -H "Authorization: Bearer {your_workspace_token}" \
  -H "Content-Type: application/json" \
  -d '{
    "service": "slack",
    "reason": "Need to post high-score lead notifications to #leads channel for team review"
  }'

# Request CRM grant (example: HubSpot)
curl -X POST https://api.usezombie.com/v1/workspaces/{workspace_id}/zombies/{zombie_id}/integration-requests \
  -H "Authorization: Bearer {your_workspace_token}" \
  -d '{
    "service": "hubspot",
    "reason": "Need to create and update contact records for extracted leads"
  }'
```

You will receive a Slack DM (or Discord, or dashboard notification) like:

```
🔐 Lead Collector is requesting Slack access

Reason: "Need to post high-score lead notifications to #leads channel"
Scopes: Full access (*)

[Approve] [Deny]
```

Click **Approve**. The zombie is now live.

### Step 4: Verify grants

```bash
zombiectl grant list --zombie {zombie_id}

# Expected output:
# SERVICE     SCOPES  STATUS    APPROVED_AT
# slack       *       approved  2026-04-12T10:30:00Z
# hubspot     *       approved  2026-04-12T10:31:00Z
```

---

## Runtime — what the zombie does on each email

When an email arrives at the configured inbox, UseZombie starts the Lead Collector Zombie
with the email as input. The zombie:

1. **Reads the email** (no UseZombie proxy needed — email content is in the trigger payload)

2. **Extracts structured lead data:**
   ```json
   {
     "name": "Jane Smith",
     "email": "jane@acme.com",
     "company": "Acme Corp",
     "intent": "pricing inquiry",
     "message_summary": "Interested in enterprise tier, team of 50"
   }
   ```

3. **Scores the lead** (reasoning step, no external call)

4. **Writes to CRM** via UseZombie execute:
   ```
   POST /v1/execute
   {target: "api.hubspot.com/crm/v3/contacts", credential_ref: "hubspot", body: {...}}
   ```
   UseZombie checks grant → injects HubSpot key → proxies → lead created in CRM.

5. **Routes based on score:**
   - Score < 60: Silent CRM write only
   - Score 60–89: Posts to Slack `#leads`:
     ```
     POST /v1/execute
     {target: "slack.com/api/chat.postMessage", credential_ref: "slack",
      body: {channel: "#leads", text: "New lead: Jane Smith, Acme Corp — 72/100"}}
     ```
   - Score ≥ 90: Triggers human approval gate before Slack post:
     ```
     UseZombie → Slack DM to workspace owner:
     "Lead Collector wants to tag @sales-lead for Jane Smith (94/100, enterprise) — Approve?"
     [Approve] [Deny]
     ```

---

## Approval gate configuration

Hot lead actions (score ≥ 90, enterprise tag, direct response to sender) require
human approval. Configure the firewall rules for your zombie:

```bash
zombiectl zombie firewall set --zombie {zombie_id} --config '{
  "endpoint_rules": [
    {
      "method": "POST",
      "domain": "slack.com",
      "path": "/api/chat.postMessage",
      "action": "requires_approval",
      "condition": "body.text contains @sales-lead"
    }
  ]
}'
```

---

## Zombie-to-zombie handoff (optional)

If you have a Lead Researcher Zombie, you can chain them:

```bash
zombiectl zombie chain \
  --from {lead_collector_zombie_id} \
  --to {lead_researcher_zombie_id} \
  --on "lead.score >= 60"
```

The Lead Researcher receives the structured lead as input and runs enrichment
(LinkedIn lookup, company size, recent funding) before the CRM write.

---

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `UZ-GRANT-001` on execute | No approved grant for service | Run grant request, wait for approval |
| `UZ-GRANT-002` on execute | Grant pending approval | Approve in Slack/Discord/dashboard |
| `UZ-TOOL-001` on execute | Credential not in vault | `zombiectl credential add {service}` |
| `UZ-FW-001` on execute | Domain not in allowlist | Add domain to zombie's allowed list |
| No webhook firing | Webhook URL misconfigured | Verify in AgentMail/Gmail settings |

---

## External agent variant (Path B)

If your lead collection pipeline runs outside UseZombie (e.g. a LangGraph graph
processing emails from your own infra), use an external agent key:

```bash
zombiectl agent create --workspace {ws} --name "langgraph-lead-pipeline"
# Save the zmb_ key returned — shown once only

# In your LangGraph code:
import requests
response = requests.post(
    "https://api.usezombie.com/v1/execute",
    headers={"Authorization": "Bearer zmb_your_key_here"},
    json={
        "zombie_id": "{zombie_id}",
        "target": "api.hubspot.com/crm/v3/contacts",
        "method": "POST",
        "credential_ref": "hubspot",
        "body": {"properties": {"email": "jane@acme.com", "firstname": "Jane"}}
    }
)
```

The integration grant system applies identically — the external agent still needs
an approved grant for `hubspot` before UseZombie injects the key.
