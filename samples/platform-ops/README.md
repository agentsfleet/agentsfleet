# Platform Ops Agent

The flagship sample. A markdown-defined agent that diagnoses fly.io +
upstash failures (and optionally GitHub Actions failures), posts a
plain-prose summary to Slack, and stays read-only on every input.
The authoritative behaviour lives in `SKILL.md` (decisioning prose,
including the GitHub-webhook handling) and `TRIGGER.md` (tools,
network allowlist, budget). This README is just the operator
quick-start.

```
   chat / cron / GH webhook
            │
            ▼
   ┌────────────────────────┐         read-only
   │  agentsfleetd worker +      │ ─────▶  fly.io + upstash
   │  runner (LLM)          │
   │                        │ ─────▶  Slack #platform-ops
   │  reads SKILL.md +      │         (one plain-prose post)
   │  TRIGGER.md            │
   └────────────────────────┘
```

## Quick start

Prereqs: `agentsfleet` installed (`npm install -g @agentsfleet/agentctl`),
a signed-in Clerk tenant, a fly.io PAT with read scope, an
upstash account-management API token, and a Slack bot (`chat:write`)
already invited to the channel you want posts in.

```bash
# 1. Vault credentials — opaque JSON keyed by name, never seen by the agent.
agentsfleet credential add fly     --data='{"host":"api.machines.dev","api_token":"<fly-pat>"}'
agentsfleet credential add upstash --data='{"host":"api.upstash.com","api_token":"<mgmt-token>"}'
agentsfleet credential add slack   --data='{"host":"slack.com","bot_token":"<xoxb-...>"}'

# 2. Install. Output: a agent id.
agentsfleet install --from samples/platform-ops

# 3. Chat.
agentsfleet chat <agent_id>
> morning health check
```

Secret substitution happens at the tool bridge *after* the sandbox
closes; the agent's LLM context only ever sees `${secrets.x.y}`
placeholders. A missing credential at tool-call time emits a single
`UZ-GRANT-001` event pointing at the exact `credential add` line to
run — clean halt, no partial Slack post.

## Optional: GitHub Actions trigger

Add a `github` credential and register a webhook on the repo:

```bash
openssl rand -base64 32                                            # paste this into GitHub's webhook UI as the Secret
agentsfleet credential add github --data='{"webhook_secret":"<above>","api_token":"<gh-pat>"}'
```

In the repo's *Settings → Webhooks*: payload URL
`https://api.agentsfleet.net/v1/webhooks/<agent_id>/github`, content
type `application/json`, events *Workflow runs*. The next failed
`workflow_run` lands within ~100 ms; the agent reads run logs,
cross-references recent commits, posts to Slack. Successes and other
event types are filtered at the receiver; GitHub retries (up to 72 h
on the same delivery UUID) are deduped server-side. The agent's
prose for this path lives in `SKILL.md` under "When the trigger is a
GitHub Actions failure."

A one-command install skill (`/agentsfleet-install-platform-ops`) is
in design — it will generate the secret, store both creds, and print
the GitHub config for you in one go.

## Limitations (v2.0-alpha)

- One Slack channel per agent; plain-text posts only (no blocks).
- Read-only diagnosis — remediation lives in separate approval-gated
  agents.
- Budget envelope ~$8/month per agent.

## Related

- `SKILL.md` — agent prose (the authoritative behaviour, including
  webhook decisioning).
- `TRIGGER.md` — tools, network allowlist, budget.
- `docs/ARCHITECTURE_AGENT_EVENT_FLOW.md` — install / chat / kill
  sequence diagrams.
