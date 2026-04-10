---
name: lead-collector
trigger:
  type: webhook
  source: agentmail
  event: message.received
skills:
  - agentmail
credentials:
  - agentmail_api_key
network:
  allow:
    - api.agentmail.to
budget:
  daily_dollars: 5.00
  monthly_dollars: 29.00
---
