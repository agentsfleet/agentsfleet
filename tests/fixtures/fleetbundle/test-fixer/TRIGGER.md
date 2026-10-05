---
name: test-fixer

x-agentsfleet:
  triggers:
    - type: api
  tools:
    - shell
    - apply_patch
    - git
  credentials:
    - github
  repositories:
    - agentsfleet/greeter
  repository_access: read
  budget:
    daily_dollars: 2.00
    monthly_dollars: 20.00
---

# Install binding

A fixture for the daemon's integration lane. The runner checks
`agentsfleet/greeter` out into the workspace before the turn, and the fleet
works it with the sandbox-side tools alone: it declares no network, so
nothing it runs reaches past the sandbox.
