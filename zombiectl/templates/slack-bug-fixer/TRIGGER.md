---
name: slack-bug-fixer
trigger:
  type: webhook
  source: slack
  event: message
credentials:
  - slack_bot_token
  - slack_signing_secret
  - github_token
skills:
  - slack
  - git
  - github
budget:
  daily_dollars: 10.0
  monthly_dollars: 50.0
network:
  allow:
    - api.slack.com
    - api.github.com
    - github.com
---

## Trigger Logic

This zombie activates on Slack message events from the configured channel.
The webhook signature is verified using the slack_signing_secret credential.
Messages from bots are ignored to prevent feedback loops.

## Security

- Slack signature verification (HMAC-SHA256) on every webhook
- Bearer token required (auto-generated on `zombiectl up`)
- Network restricted to Slack and GitHub APIs
- Approval gate fires before push to main (requires M4)
- Budget hard-capped at $50/month
