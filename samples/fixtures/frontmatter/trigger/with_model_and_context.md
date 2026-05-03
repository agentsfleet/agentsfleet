---
name: with-model-and-context-skill

x-usezombie:
  model: accounts/fireworks/models/kimi-k2.6
  context:
    context_cap_tokens: 256000
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
  budget:
    daily_dollars: 1.0
---
