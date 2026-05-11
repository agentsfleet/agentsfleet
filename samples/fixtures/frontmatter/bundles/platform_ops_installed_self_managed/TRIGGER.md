---
name: platform-ops-zombie

x-usezombie:
  model: ""
  context:
    context_cap_tokens: 0
    tool_window: auto
    memory_checkpoint_every: 5
    stage_chunk_threshold: 0.75

  trigger:
    type: webhook
    source: github
    signature:
      secret_ref: github_secret
      header: x-hub-signature-256
      prefix: "sha256="

  tools:
    - http_request
    - memory_recall
    - memory_store

  credentials:
    - fly
    - upstash
    - slack
    - github

  network:
    allow:
      - api.machines.dev
      - api.upstash.com
      - slack.com
      - api.github.com

  budget:
    daily_dollars: 1.00
    monthly_dollars: 8.00
---
