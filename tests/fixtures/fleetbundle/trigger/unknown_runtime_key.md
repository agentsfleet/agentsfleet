---
# Typo: `contxt` instead of a known runtime key. Rigid validation under
# x-agentsfleet: rejects with UnknownRuntimeKey.
name: typo-skill

x-agentsfleet:
  triggers:
    - type: cron
      schedule: "0 9 * * *"
  tools:
    - agentmail
  budget:
    daily_dollars: 1.0
  contxt:
    foo: bar
---
