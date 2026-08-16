---
name: steer-probe
x-agentsfleet:
  model: "{{model}}"
  context:
    context_cap_tokens: {{context_cap_tokens}}
  triggers:
    - type: api
  tools: []
  credentials: []
  network:
    allow:
      - api.fireworks.ai
  budget:
    daily_dollars: 1.00
---

# Manual steer only

This Fleet has no provider trigger. Acceptance wakes it with `agentsfleet steer`.
