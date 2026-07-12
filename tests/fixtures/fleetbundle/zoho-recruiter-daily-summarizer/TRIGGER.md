---
name: zoho-recruiter-daily-summarizer
x-agentsfleet:
  triggers:
    - type: cron
      schedule: "0 9 * * 1-5"
  tools:
    - http_request
  credentials:
    - zoho_recruit
  network:
    allow:
      - recruit.zoho.com
      - accounts.zoho.com
  budget:
    daily_dollars: 1.0
---
# Wake rule

Wakes weekdays at 09:00 (cron) to summarize the hiring pipeline's movement since
the previous digest.
