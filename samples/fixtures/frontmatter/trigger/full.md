---
name: full-skill

x-usezombie:
  trigger:
    type: webhook
    source: github
    signature:
      secret_ref: github_secret
      header: x-hub-signature-256
      prefix: "sha256="
  tools:
    - agentmail
    - slack
    - github
  credentials:
    - github_token
    - slack_bot_token
  network:
    allow:
      - api.github.com
      - slack.com
  budget:
    daily_dollars: 1.0
    monthly_dollars: 8.0
---

# Operator commentary section

Anything below the closing `---` is body prose for human operators —
the runtime does not parse it. Use this space for credential-shape
documentation, budget rationale, firewall reasoning, etc.
