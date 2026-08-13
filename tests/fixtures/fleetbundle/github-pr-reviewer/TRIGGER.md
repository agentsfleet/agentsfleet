---
name: github-pr-reviewer
x-agentsfleet:
  triggers:
    - type: webhook
      source: github
      events:
        - pull_request
  tools:
    - http_request
  credentials:
    - github
  # Repository EGRESS binding — which repositories this fleet's minted token may
  # reach, and how far. Distinct from a webhook trigger's `repositories`, which
  # is an INGRESS binding naming what may WAKE the fleet. Both keys are required
  # together: a fleet declaring neither mints nothing, because an unbound mint
  # would carry the App installation's full permissions across every repository
  # it covers. Reviewing a Pull Request needs write (it posts review comments).
  repositories:
    - agentsfleet/agentsfleet
  repository_access: write
  repository_base: main
  network:
    allow:
      - api.github.com
  budget:
    daily_dollars: 2.0
---
# Wake rule

Wakes on GitHub `pull_request` webhook events for the connected repository.
