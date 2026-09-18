---
name: github-pr-reviewer
x-agentsfleet:
  triggers:
    - type: webhook
      source: github
      events:
        - pull_request
      # Repository INGRESS binding — which repositories may WAKE this fleet.
      # Required for managed App delivery: one App delivery is offered to every
      # fleet in the workspace, so a trigger naming no repository has subscribed
      # to nothing and is never woken. `Binding::serves_repository` reads THIS
      # key, not the egress one below, and answers false when it is absent.
      repositories:
        - agentsfleet/linkwarden
  tools:
    - http_request
  credentials:
    - github
  # Repository EGRESS binding — which repositories this fleet's minted token may
  # reach, and how far. Distinct from the webhook trigger's `repositories` above,
  # which is the INGRESS binding. Both are required: a fleet declaring neither
  # mints nothing, because an unbound mint would carry the App installation's
  # full permissions across every repository it covers. Reviewing a Pull Request
  # needs write (it posts review comments).
  repositories:
    - agentsfleet/linkwarden
  repository_access: write
  repository_base: main
  network:
    allow:
      - api.github.com
  budget:
    daily_dollars: 2.0
---
# Wake rule

Wakes on GitHub `pull_request` webhook events for the repositories named in the
trigger's `repositories` list.
