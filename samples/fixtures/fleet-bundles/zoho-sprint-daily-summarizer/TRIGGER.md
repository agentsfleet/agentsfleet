---
name: zoho-sprint-daily-summarizer
x-agentsfleet:
  triggers:
    - type: cron
      schedule: "0 18 * * 1-5"
  tools:
    - zoho_sprint_read
  credentials:
    - zoho
  network:
    allow:
      - sprintsapi.zoho.com
      - accounts.zoho.com
  budget:
    daily_dollars: 1.0
---
# Wake rule

Wakes weekdays at 18:00 (cron) to summarize the day's Zoho Sprints activity.
