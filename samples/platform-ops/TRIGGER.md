---
name: platform-ops-zombie

x-usezombie:
  # Both fields are populated by `/usezombie-install-platform-ops` from the
  # `tenant_provider` block of `zombiectl doctor --json`. Under the platform
  # default they carry resolved values (e.g. Fireworks Kimi K2.6 + 256K cap);
  # under BYOK they carry the empty-string / zero sentinels and the worker
  # overlays the real values from `core.tenant_providers` at trigger time.
  model: "{{model}}"
  context:
    context_cap_tokens: {{context_cap_tokens}}
    tool_window: auto
    memory_checkpoint_every: 5
    stage_chunk_threshold: 0.75

  trigger:
    type: webhook
    source: github
    # Default credential lookup: vault `github`, field `webhook_secret`. The
    # install-skill rewrites `credential_name:` to `github-{zombie_slug}` only
    # when the operator picks per-zombie scoping at second-install time.
    signature:
      secret_ref: github_secret
      header: x-hub-signature-256
      prefix: "sha256="

  tools:
    - http_request
    - memory_recall
    - memory_store
    - cron_add
    - cron_list
    - cron_remove

  credentials:
    - fly
    - upstash
    - slack
    - github
    # Credential shapes — see README for the `zombiectl credential add` flag form.
    # fly      = { host: "api.machines.dev", api_token: "<fly token>" }
    # upstash  = { host: "api.upstash.com",  api_token: "<upstash token>" }
    # slack    = { host: "slack.com",        bot_token: "<slack bot token>" }
    # github   = { webhook_secret: "<base64>", api_token: "<gh PAT>" }
    # Substituted into http_request at the tool-bridge as ${secrets.NAME.FIELD}
    # after the executor sandbox closes around the agent. The agent never sees
    # raw bytes — the worst a prompt-injection can make it print is the
    # placeholder string.

  network:
    allow:
      - api.machines.dev
      - api.upstash.com
      - slack.com
      - api.github.com
    # The sandbox firewall rejects any outbound HTTPS to a host not on this
    # list. If the agent invents an endpoint on an unexpected host, the call
    # fails fast and the agent reasons from the error.

  budget:
    # Two independent hard caps — the first to trip blocks further runs.
    # They do NOT compose (daily × 30 ≠ monthly); this is intentional.
    #
    # `monthly_dollars` is the real spend envelope: the $10 starter credit
    # minus $2 of headroom so a second zombie fits. At typical cost (~$0.30
    # per diagnosis) it bites after ~26 runs in a normal month.
    #
    # `daily_dollars` is a blast-radius guard, not a pro-rated monthly share
    # (which would be ~$0.27 — too tight; one normal run would trip it).
    # $1 ≈ 3 normal runs' worth; hitting it in one UTC day means something
    # is wrong (stuck reasoning, prompt injection spamming tool calls) and
    # the right move is to pause and inspect, not burn the whole month.
    daily_dollars: 1.00
    monthly_dollars: 8.00
---
