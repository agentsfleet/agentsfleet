---
name: ci-responder

x-agentsfleet:
  triggers:
    - type: api
  tools:
    - http_request
    - memory_store
    - memory_recall
  credentials:
    - github
    - grafana
  repositories:
    - agentsfleet/linkwarden
  repository_access: read
  network:
    read_only: true
    allow:
      - api.github.com
      - grafana.example.net
  budget:
    daily_dollars: 2.00
    monthly_dollars: 20.00
---

# Install binding

Replace `grafana.example.net` with the development or production stack's
public hostname before installation. Install with `--slack-channel
<CHANNEL_ID>` so the stored trigger subscribes to one channel.
