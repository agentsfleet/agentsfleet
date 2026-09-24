---
name: mention-valid

x-agentsfleet:
  triggers:
    - type: mention
      source: slack
      channels:
        - C0123456789
  tools:
    - http_request
  budget:
    daily_dollars: 1.00
---
