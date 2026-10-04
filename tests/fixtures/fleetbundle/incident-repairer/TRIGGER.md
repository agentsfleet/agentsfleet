---
name: incident-repairer

x-agentsfleet:
  triggers:
    # A CONCRETE incident wakes the repairer: a failed workflow_run delivered
    # over the signed per-fleet GitHub webhook. The responder keeps the
    # scheduled sweeps — the two triggers are DISJOINT on purpose, so which
    # member handles an event is decided by wiring, never by a picker's
    # judgment call.
    - type: webhook
      source: github
      events:
        - workflow_run
      repositories:
        - agentsfleet/agentsfleet
    # A human can also hand it an incident directly (manual steer).
    - type: api

  tools:
    # Declared explicitly — an omitted `tools` key falls back to the full
    # default set (see the responder's note; same rule).
    #
    - http_request

  credentials:
    - elastic
    - grafana
    - github
    # Same shapes as the responder's (substituted at the request boundary).
    # github is the mintable integration — and for THIS bundle the daemon
    # mints WRITE, under the standing integration grant the install wrote.
    # `agentsfleet grant delete` withdraws it before the next event runs.

  # Repository EGRESS binding — `write` is the entire point of this member,
  # and the entire cost: the standing integration grant authorises every
  # write this fleet makes, with no per-event human answer, and the
  # minted token carries `contents: write` + `pull_requests: write` for
  # exactly this repository, one hour, and NO `workflows` permission — GitHub
  # itself refuses a push into `.github/workflows/`.
  repositories:
    - agentsfleet/agentsfleet
  repository_access: write
  repository_base: main

  network:
    read_only: true
    read_post_paths:
      - https://demo.es.us-east-1.aws.elastic.cloud/_query
    allow:
      # Deployment-specific hosts, pinned by the playbook or an operator PATCH.
      - demo.es.us-east-1.aws.elastic.cloud
      - demo-grafana.internal
      - api.github.com

  budget:
    # A repair run reads telemetry, history, AND file contents, then pushes —
    # costlier than a diagnosis. Sized for ~2 full repairs a day; tripping it
    # means a loop or an injection, so it pauses rather than burns.
    daily_dollars: 3.00
    monthly_dollars: 30.00
---
# Wake rule

Wakes when the bound repository delivers a failed `workflow_run`, or when a
human steers an incident to it directly. The standing integration grant the
install wrote authorises the write; deleting that grant stops the next wake
before any run starts.
A run that ships ends with exactly one branch and one draft Pull Request; a
run that cannot fix confidently ends diagnosis-only.
