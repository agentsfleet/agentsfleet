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
  optional_cron:
    schedule: "0 9 * * *"
    # When the cron fires, the platform synthesises this message as the
    # payload — the zombie receives it the same way it would a webhook
    # POST, so the prompt's "When the operator sends a question..."
    # branch applies uniformly.
    message: "Run the daily homelab health scan: report any pods in CrashLoopBackOff, containers using >80% of their memory limit, and volumes above 85% full. Summarise in three bullets."
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
