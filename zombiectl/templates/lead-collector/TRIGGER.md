---
name: lead-collector
trigger:
  type: webhook
  source: agentmail
  event: message.received
chain:
  - lead-enricher
credentials:
  - agentmail_api_key
budget:
  daily_dollars: 5.0
  monthly_dollars: 29.0
network:
  allow:
    - api.agentmail.to
---

## Trigger Logic

This zombie activates on inbound AgentMail webhooks (message.received events).
Each incoming email is delivered to the agent as a webhook event.

## Security

- Bearer token required (auto-generated on `zombiectl up`)
- Network restricted to agentmail API only
- Budget hard-capped at $29/month
