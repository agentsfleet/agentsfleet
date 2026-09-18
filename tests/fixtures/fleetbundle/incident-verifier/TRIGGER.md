---
name: incident-verifier

x-agentsfleet:
  triggers:
    - type: webhook
      source: github
      events:
        - repair_production_result
      repositories:
        - agentsfleet/agentsfleet

  tools:
    - http_request

  credentials:
    - elastic
    - grafana
    - github

  repositories:
    - agentsfleet/agentsfleet
  repository_access: read

  network:
    read_only: true
    read_post_paths:
      - https://demo.es.us-east-1.aws.elastic.cloud/_query
    allow:
      - demo.es.us-east-1.aws.elastic.cloud
      - demo-grafana.internal
      - api.github.com

  budget:
    daily_dollars: 1.00
    monthly_dollars: 10.00
---
# Wake rule

Wakes only for a proof-qualified `repair_production_result` from GitHub. It
checks the supplied merged commit against Grafana and Elasticsearch evidence in
the supplied fixed window, then records `cleared`, `not_cleared`, or
`inconclusive` in its Fleet event response.
