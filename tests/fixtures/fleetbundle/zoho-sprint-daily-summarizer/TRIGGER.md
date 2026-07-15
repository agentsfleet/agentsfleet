---
name: zoho-sprint-daily-summarizer
x-agentsfleet:
  triggers:
    - type: cron
      schedule: "0 9 * * *"
      timezone: "Asia/Kolkata"
      message: "Summarize today's Zoho Sprints activity"
  tools:
    - http_request
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

Wakes every morning at 09:00 Asia/Kolkata to summarize the day's Zoho Sprints activity.
