---
name: github-pr-reviewer
x-agentsfleet:
  triggers:
    - type: webhook
      source: github
      events:
        - pull_request
  tools:
    - github_review_comment
  credentials:
    - github
  network:
    allow:
      - api.github.com
  budget:
    daily_dollars: 2.0
---
# Wake rule

Wakes on GitHub `pull_request` webhook events for the connected repository.
