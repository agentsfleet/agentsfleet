# Ops Zombie — Agent-Executable Integration Guide

**Published at:** docs.usezombie.com/integrations/ops-zombie
**Zombie type:** Ops / Infrastructure Monitor
**Integration actors:** Grafana (input) → Zombie (reasoning) → Slack + Discord (output)
**Human involvement:** One-time grant approval. Auto-scale and page-on-call require per-action approval.

---

## What this zombie does

The Ops Zombie watches Grafana log streams and alert webhooks. When anomalies are
detected, it:
- Classifies the event: noise / warning / critical / incident
- Posts structured alerts to Slack `#alerts` or Discord `#ops`
- For critical incidents: triggers human-in-loop approval before paging on-call
- For repeated patterns: summarizes and suppresses duplicate noise
- For auto-remediation actions (scale up, restart service): always requires approval

---

## Actors

```
[Grafana alert fires OR log anomaly detected]
    → Grafana webhook → UseZombie (webhook trigger)
      OR
    → Grafana API polling (cron-triggered zombie)
        → Ops Zombie (reasoning: classify, summarize, route)
            → Slack API (post to #alerts)       [UseZombie injects Slack token]
            → Discord API (post to #ops)        [UseZombie injects Discord token]
            → Human (approval gate)             [for page-on-call, auto-scale]
```

---

## Prerequisites

| Credential ref | What it is | How to store |
|----------------|------------|-------------|
| `grafana` | Grafana service account token | `zombiectl credential add grafana` |
| `slack` | Slack bot token (`chat:write` on `#alerts`) | `zombiectl credential add slack` |
| `discord` | Discord bot token | `zombiectl credential add discord` |

Store all that apply. The zombie uses whichever have approved grants.

---

## Setup — step by step

### Step 1: Create the zombie

```bash
zombiectl zombie create \
  --name "Ops Zombie" \
  --description "Watches Grafana alerts and log streams, routes to Slack and Discord"
```

Note the `zombie_id`.

### Step 2A: Configure Grafana alert webhook (push model)

In Grafana: **Alerting → Contact points → New contact point**
- Type: Webhook
- URL: `https://api.usezombie.com/v1/webhooks/{zombie_id}`
- Method: POST

Add to a Notification Policy so your alert rules route here.

### Step 2B: Configure cron polling (pull model, for log stream monitoring)

```bash
# Poll Grafana Loki logs every 5 minutes
zombiectl zombie schedule \
  --zombie {zombie_id} \
  --cron "*/5 * * * *" \
  --input '{
    "task": "poll_grafana_logs",
    "query": "{job=\"api-server\"} |= \"error\" | rate[5m] > 10"
  }'
```

Use push for alert rules, pull for proactive log anomaly detection.

### Step 3: Request integration grants

```bash
# Grafana grant — for querying logs and fetching alert context
curl -X POST https://api.usezombie.com/v1/workspaces/{workspace_id}/zombies/{zombie_id}/integration-requests \
  -H "Authorization: Bearer {your_workspace_token}" \
  -d '{
    "service": "grafana",
    "reason": "Need to query Loki log streams and fetch alert context to classify incidents"
  }'

# Slack grant — for posting classified alerts
curl -X POST https://api.usezombie.com/v1/workspaces/{workspace_id}/zombies/{zombie_id}/integration-requests \
  -H "Authorization: Bearer {your_workspace_token}" \
  -d '{
    "service": "slack",
    "reason": "Need to post structured incident alerts to #alerts channel"
  }'

# Discord grant — if using Discord
curl -X POST https://api.usezombie.com/v1/workspaces/{workspace_id}/zombies/{zombie_id}/integration-requests \
  -H "Authorization: Bearer {your_workspace_token}" \
  -d '{
    "service": "discord",
    "reason": "Need to post alerts to #ops channel on Discord"
  }'
```

Approve each via Slack DM, Discord, or dashboard.

### Step 4: Configure approval gate for high-impact actions

```bash
zombiectl zombie firewall set --zombie {zombie_id} --config '{
  "endpoint_rules": [
    {
      "method": "POST",
      "domain": "slack.com",
      "path": "/api/chat.postMessage",
      "action": "requires_approval",
      "condition": "body.text contains @on-call"
    },
    {
      "method": "POST",
      "domain": "discord.com",
      "path": "/api/v10/channels/*/messages",
      "action": "requires_approval",
      "condition": "body.content contains @oncall"
    }
  ]
}'
```

---

## Runtime — what the zombie does on each alert

### Push model (Grafana alert fires):

1. **Receives alert payload** from Grafana webhook
2. **Fetches log context** (if needed for classification):
   ```
   POST /v1/execute
   {
     target: "your-grafana.com/api/ds/query",
     credential_ref: "grafana",
     body: {queries: [{expr: "{job=\"api-server\"} |= \"error\"", maxLines: 50}]}
   }
   ```
   UseZombie checks Grafana grant → injects service account token → fetches logs.

3. **Classifies the event:**
   - `noise`: alert fired but logs show normal pattern → suppresses
   - `warning`: elevated error rate, not yet critical → posts to Slack
   - `critical`: service degraded, SLA at risk → posts to Slack + Discord
   - `incident`: service down → posts everywhere + triggers on-call approval

4. **Posts to Slack:**
   ```
   POST /v1/execute
   {
     target: "slack.com/api/chat.postMessage",
     credential_ref: "slack",
     body: {
       channel: "#alerts",
       blocks: [
         {"type": "header", "text": "⚠️ Warning: API error rate elevated"},
         {"type": "section", "text": "Error rate: 8.3% (threshold: 5%)\nService: api-server\nLast 5m: 142 errors"},
         {"type": "section", "text": "Top error: `connection timeout to postgres` (87 occurrences)"}
       ]
     }
   }
   ```

5. **For critical/incident — on-call approval:**
   ```
   UseZombie → Slack DM to workspace owner:
   "Ops Zombie wants to page @on-call for api-server incident (P1) — Approve?"
   [Approve] [Deny]
   ```
   On approve: message with `@on-call` tag sent.

### Pull model (cron-based log polling):

Same classification flow, but triggered on schedule rather than alert webhook.
Useful for detecting slow-burn anomalies that don't cross Grafana alert thresholds.

---

## Alert format reference

**Warning (Slack):**
```
⚠️ Warning: {service} — {metric} elevated
Error rate: {value}% (threshold: {threshold}%)
Duration: {duration}
Top error: {error_message} ({count} occurrences)
Runbook: {link if configured}
```

**Critical (Slack + Discord):**
```
🔴 Critical: {service} degraded
SLA impact: Yes / Likely
Error rate: {value}%
Affected endpoints: {list}
Suggested action: {reasoning step output}
```

**Incident (all channels + approval gate):**
```
🚨 Incident: {service} DOWN
Duration: {elapsed}
Last healthy checkpoint: {timestamp}
Requesting on-call page — awaiting approval
```

---

## Noise suppression

The zombie tracks seen alerts in session context. Identical alert types within a
60-minute window are suppressed with a counter update instead of a new post:

```
[existing Slack thread updated]
  ⚠️ Warning: api-server — error rate elevated (now 23 occurrences, last: 2m ago)
```

This prevents alert fatigue during sustained incidents.

---

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| Grafana webhook not reaching zombie | URL misconfigured in Grafana | Verify Contact Point URL in Grafana Alerting |
| `UZ-GRANT-001` on Grafana query | No Grafana grant | Request grant, approve in Slack/Discord |
| Alerts posting but no Grafana context | Grafana credential missing or expired | `zombiectl credential add grafana` |
| On-call pings not going through | Approval gate blocking `@on-call` | Approve the pending gate in Slack/dashboard |
| Too many alerts (noise) | Classification threshold too low | Adjust in zombie config or tune Grafana alert rules |

---

## External agent variant (Path B)

If your observability pipeline runs in a separate system (e.g. a Python script
consuming OTel streams and sending to UseZombie for notification):

```bash
zombiectl agent create --workspace {ws} --name "otel-ops-agent"
# Save the zmb_ key

# In your Python monitoring script:
import requests

def alert_via_usezombie(severity: str, message: str, service: str):
    response = requests.post(
        "https://api.usezombie.com/v1/execute",
        headers={"Authorization": "Bearer zmb_your_key_here"},
        json={
            "zombie_id": "{zombie_id}",
            "target": "slack.com/api/chat.postMessage",
            "method": "POST",
            "credential_ref": "slack",
            "body": {
                "channel": "#alerts",
                "text": f"[{severity}] {service}: {message}"
            }
        }
    )
    return response.json()
```

The integration grant for `slack` still applies — the external agent must have an
approved grant before UseZombie injects the Slack token.
