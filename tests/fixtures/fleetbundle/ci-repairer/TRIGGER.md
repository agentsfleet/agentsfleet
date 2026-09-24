---
name: ci-repairer

x-agentsfleet:
  triggers:
    - type: api
  tools:
    - http_request
  credentials:
    - github
  repositories:
    - agentsfleet/linkwarden
  repository_access: write
  repository_base: dev
  network:
    allow:
      - api.github.com
  budget:
    daily_dollars: 3.00
    monthly_dollars: 30.00
---

# Install binding

Install with `--slack-channel <CHANNEL_ID>`. The install adds this fleet's
single-channel mention trigger. A write-bound fleet is addressed only; an
unaddressed mention cannot wake it.
