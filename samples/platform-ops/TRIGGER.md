---
name: platform-ops-zombie
trigger:
  type: chat
  # Chat is the one operator-initiated input channel. `zombiectl chat`
  # and the UI chat widget both hit POST /v1/.../zombies/{id}/steer; the
  # zombie thread's top-of-loop poller converts the steer key into a stream
  # event. There is no webhook payload for this zombie, so no payload_schema.
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
  # Credential shapes — see README for the `zombiectl credential add` flag form.
  # fly       = { host: "api.machines.dev", api_token: "<fly token>" }
  # upstash   = { host: "api.upstash.com",  api_token: "<upstash token>" }
  # slack     = { host: "slack.com",        bot_token: "<slack bot token>" }
  # Substituted into http_request at the tool-bridge as ${secrets.NAME.FIELD}
  # after the executor sandbox closes around the agent. The agent never sees
  # raw bytes — the worst a prompt-injection can make it print is the
  # placeholder string.
network:
  allow:
    - api.machines.dev
    - api.upstash.com
    - slack.com
  # The sandbox firewall rejects any outbound HTTPS to a host not on this
  # list. If the agent invents an endpoint on an unexpected host, the call
  # fails fast and the agent reasons from the error.
budget:
  # Starter-credit envelope: $10 total, aim to keep platform-ops under $8/mo
  # so the operator has headroom for a second zombie. `daily_dollars` is a
  # tighter blast-radius guard against runaway agent loops.
  daily_dollars: 1.00
  monthly_dollars: 8.00
---
