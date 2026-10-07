---
name: delegating-triager

x-agentsfleet:
  triggers:
    - type: api
  tools:
    - delegate
    - file_read
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
hands each file to a child loop that reads it with `file_read` and answers;
it declares no network, so nothing it runs reaches past the sandbox.
