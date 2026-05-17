---
name: minimal-skill

x-usezombie:
  triggers:
    - type: cron
      schedule: "0 9 * * *"
  tools:
    - agentmail
  budget:
    daily_dollars: 1.0
---
