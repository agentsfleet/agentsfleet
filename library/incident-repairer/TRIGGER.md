---
name: incident-repairer

x-agentsfleet:
  triggers:
    # Woken by a message, never by a clock and never by a webhook. Today a HUMAN
    # sends that message, from the diagnosis the investigator posted to Slack.
    # The investigator cannot send it itself: waking a fleet and reconfiguring
    # one are one scope today (`fleet:write`), so a credential able to wake this
    # fleet is also able to PATCH the `gates` block below to empty — after which
    # nothing is ever asked. Splitting a message scope out of the write scope is
    # a PREREQUISITE for waking this fleet automatically, not an enhancement.
    - type: api

  tools:
    # Declared explicitly. An omitted or non-array `tools` key does NOT mean "no
    # tools" — `runner_helpers` falls back to the full default set, so the
    # surface a fleet has would depend on a field nobody wrote.
    - repo_fetch
    - git
    - http_request
    - memory_store
    - memory_recall

  credentials:
    - github
    # github = mintable integration — the daemon mints a short-lived
    # installation token at the bridge, scoped to the repositories below at the
    # access level declared below. Nothing is stored, and no token is ever
    # pasted into a workspace secret. Reaches the run as ${secrets.github.token};
    # a mintable credential answers only that one field.

  # Repository EGRESS binding — which repositories this fleet's minted token may
  # reach, and how far. Distinct from a webhook trigger's `repositories`, which
  # is an INGRESS binding naming what may WAKE a fleet. Both keys are required
  # together: declaring neither mints nothing, because an unbound mint would
  # carry the App installation's full permissions across every repository it
  # covers. `write` is what opening a Pull Request needs; it is also the reason
  # every run of this fleet is gated below.
  #
  # Deployment-specific, like the hosts: the demo playbook (or the operator, via
  # a fleet PATCH after install) pins the real repository here.
  repositories:
    - agentsfleet/agentsfleet
  repository_access: write

  network:
    allow:
      - api.github.com

  gates:
    # NON-EMPTY BY NECESSITY. `approval_gate` falls through to `.auto_approve`
    # when no rule matches, so an omitted or non-matching rule does not mean
    # "ask about nothing" — it means an autonomous agent holding a write token.
    #
    # The wildcard is deliberate and is not laziness. The pre-lease gate matches
    # a rule's `tool` against the event's TYPE and its `action` against the
    # event's ACTOR — not against a tool call. A tool-shaped rule (`tool: git`,
    # `action: push`) therefore never matches here and silently auto-approves.
    # `*`/`*` is the shape that says what this fleet actually means: every run
    # of the repairer waits for a human, because approval is what RELEASES the
    # run rather than what permits one step inside it.
    rules:
      - tool: "*"
        action: "*"
        behavior: approve
        gate_kind: repair
        blast_radius: one draft revert Pull Request on the declared repository, pushed to a new branch and merged by nobody
    # 30 minutes. An unanswered repair expires rather than lingering: the
    # incident is still broken either way, and a parked approval blocks this
    # fleet's whole queue behind it.
    timeout_ms: 1800000

  budget:
    # A repair is one bounded run: fetch a tree, compute a revert, open one
    # draft Pull Request. It costs far less than an investigation, and it only
    # runs after a human said yes — so a daily figure this size means something
    # is waking it far more often than incidents happen.
    daily_dollars: 1.00
    monthly_dollars: 10.00
---
# Wake rule

Woken by a message naming a repository, a suspect commit, and the evidence that
implicated it — in this workstream, sent by a human who read the investigator's
diagnosis in Slack.

Every wake parks on a human approval before a lease is issued. Approving
authorises **one bounded repairer run**, not specific bytes: the draft Pull
Request is the review surface where the diff is actually read.

A wake that names no repository, no commit, or a repository outside the binding
above is refused before any network call — the runner checks the ask against the
binding, and the mint is scoped by that same binding.
