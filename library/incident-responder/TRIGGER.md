---
name: incident-responder

x-agentsfleet:
  triggers:
    - type: cron
      schedule: "*/15 * * * *"
      timezone: "Etc/UTC"
      message: "Sweep the telemetry for new incidents"

  tools:
    # Declared explicitly. An omitted or non-array `tools` key does NOT mean
    # "http_request only" — `runner_helpers` falls back to the full default set,
    # so the surface a fleet has would depend on a field nobody wrote.
    #
    # The memory pair is load-bearing, not a convenience: an incident stays
    # broken until a human acts on the diagnosis, so every sweep re-finds it.
    # Without a record of what has already been escalated, one incident
    # produces the same escalation again every sweep interval.
    - http_request
    - memory_store
    - memory_recall

  credentials:
    - elastic
    - grafana
    - github
    - jira
    - slack
    # Credential shapes, substituted at the tool bridge as ${secrets.NAME.FIELD}.
    # Every value is a header-ready string: substitution happens at the request
    # boundary, so anything needing encoding must be stored already encoded.
    # elastic = { host: "<deployment>.es.<region>.aws.elastic.cloud",
    #             api_key: "<the ENCODED api key Elastic hands you, not the id>" }
    # grafana = { host: "<grafana host>", token: "<service-account token>" }
    # github  = mintable integration — the daemon mints a short-lived
    #           installation token at the bridge. Nothing is stored, and no
    #           token is ever pasted into a workspace secret.
    # jira    = { host: "<site>.atlassian.net",
    #             basic_auth: "<base64 of email:api_token>" }
    # slack   = { host: "slack.com", bot_token: "<bot token>" }

  # Repository EGRESS binding — which repositories this fleet's minted token may
  # reach, and how far. Distinct from a webhook trigger's `repositories`, which
  # is an INGRESS binding naming what may WAKE a fleet. Both keys are required
  # together: declaring neither mints nothing at all, because an unbound mint
  # would carry the App installation's full permissions across every repository
  # it covers.
  #
  # `read` is the boundary this whole crew is built on. The investigator MUST
  # reach GitHub — it cannot name a suspect commit without reading commit
  # history and verifying a branch head — but the token it is handed carries
  # `contents: read` and no `pull_requests` permission, so it cannot open a Pull
  # Request no matter what its prompt is talked into. The vendor refuses the
  # write. That is a property of the credential the daemon mints, not of the
  # wording below.
  #
  # Deployment-specific, like the hosts: the demo playbook (or the operator, via
  # a fleet PATCH after install) pins the real repository here.
  repositories:
    - agentsfleet/agentsfleet
  repository_access: read

  network:
    allow:
      # The Elastic, Grafana, and Jira hosts are deployment-specific: the demo
      # playbook (or the operator, via a fleet PATCH after install) pins the
      # real hosts here. The sandbox refuses any host not on this list, so an
      # unpinned entry fails fast rather than leaking a request elsewhere.
      - demo.es.us-east-1.aws.elastic.cloud
      - demo-grafana.internal
      - demo.atlassian.net
      - api.github.com
      - slack.com

  budget:
    # Two independent hard caps — the first to trip blocks further runs.
    # A sweep that finds nothing is cheap; a full investigation with trace
    # reads and history correlation costs more than a platform-ops diagnosis,
    # so the daily guard is sized for ~3 deep investigations. Hitting it in
    # one day means a stuck loop or an injection spamming tool calls — pause
    # and inspect rather than burn the month.
    daily_dollars: 2.00
    monthly_dollars: 20.00
---
# Wake rule

Wakes every fifteen minutes to sweep the instrumented workload's telemetry.
A quiet sweep ends silently; an incident produces a diagnosis, and a
code-shaped incident with a small confident fix ends the diagnosis with one
bounded forward-fix repair intent for a human to act on.
