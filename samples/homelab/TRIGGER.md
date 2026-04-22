---
name: homelab-zombie
trigger:
  type: webhook
  payload_schema:
    message:
      type: string
      min: 1
      max: 4000
      description: The operator's question in natural language.
  optional_cron: "0 9 * * *"
tools:
  - kubectl
  - docker
credentials:
  - kubectl_config
  - docker_socket
network:
  allow:
    - kubernetes.default.svc
    - "*.svc.cluster.local"
budget:
  daily_dollars: 2.00
  monthly_dollars: 20.00
---
