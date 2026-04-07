# UseZombie v2 — Runtime, Workspaces & Go-To-Market

**Part of:** [UseZombie v2 Overview](usezombie-v2-overview.md)

---

## What UseZombie Is

A runtime for always-on agents.

You write your agent. You package it as a UseZombie skill (or install one from ClawHub). You tell UseZombie which services it needs (email, Slack, GitHub, Jira). UseZombie runs it in a sandboxed process with:

- **Webhooks wired** — your agent gets events from email, Slack, GitHub, etc. without you setting up ngrok or a custom server.
- **Credentials hidden** — your agent never sees a token. It makes normal HTTP requests. UseZombie's firewall layer intercepts outbound traffic, injects the right credential per-request, and strips it from the agent's view.
- **Walls up** — the agent can only reach the services you declared. Everything else is blocked.
- **AI firewall** — prompt injection detection, policy enforcement, content inspection, hallucination kill on every request. See [AI Firewall](usezombie-v2-firewall.md).
- **Logs of everything** — every request the agent makes, every webhook it receives, every credential used, timestamped and replayable.
- **Spend ceiling** — per-run token budgets, wall time limits, kill-on-exceed. One bad prompt never becomes an infinite burn.
- **A kill switch** — stop any agent from the web UI or CLI, mid-action.
- **A chat panel** — talk to your running agent. "How many leads today?" / "Pause replies." / "Show me the last 5 actions."

**What it is NOT:**
- Not an agent framework (use Claude, GPT, LangChain — whatever you want).
- Not a sandbox-as-a-service (E2B does that — UseZombie is opinionated about credential isolation and always-on operation, not generic code execution).
- Not a SaaS tool with built-in AI (Instantly, Smartlead do that — UseZombie runs YOUR agent).
- Not a workflow orchestrator (Temporal, Inngest do that — UseZombie doesn't chain steps, it runs your agent process).
- Not Composio (they do auth well — UseZombie is the full runtime: auth + sandbox + firewall + webhooks + audit + kill switch).

---

## Skills Are Tools, Not Agents

Skills are capabilities the platform provides. An agent picks skills from this menu. The harness (runtime config) says which ones it's allowed to use.

```
Skills (tool adapters on ClawHub):
├── agentmail    — send/receive email via agentmail API
├── slack        — read/post messages, threads, reactions
├── github       — PRs, issues, reviews, comments, webhooks
├── git          — clone, branch, commit, push
├── linear       — create/update tickets
├── cloudflare   — tunnel status, DNS, zone health
├── pagerduty    — incidents, alerts, escalation
└── ...future    — jira, notion, discord, etc.
```

The agent's intelligence comes from its **instructions** (the LLM prompt in the skill spec). The skills just give it hands.

### Skill spec format (markdown with frontmatter)

```markdown
---
name: lead-collector
description: Reply to inbound signups with invite codes
version: 0.1.0
skills: [agentmail]
trigger:
  type: webhook
  source: agentmail
  event: message.received
credentials:
  agentmail:
    inject_for: [api.agentmail.to]
network:
  allow: [api.agentmail.to]
budget:
  max_tokens: 50000
  max_wall_time: 120s
---

## Instructions

You are a lead collector for {{workspace.name}}.

When an email arrives:
1. Parse the sender's email and name
2. Generate a unique invite code
3. Reply with the welcome template
4. Log the lead to the activity stream

## On error

If agentmail is unreachable, retry 3 times then log the failure.
Do not send duplicate replies — check the activity stream first.
```

The frontmatter is the machine-readable policy. The markdown body is the agent's instructions. One file. No separate YAML. Operators who use kogo.ai never see this file — it's generated from the UI. Developers who use UseZombie directly write it.

---

## Workspaces: Multi-Agent Workflows

A workspace is a **project** — a group of agents that share an activity stream and can be managed together. One workspace = one use case, multiple agents working together.

The workspaces below are **sample use cases, not the product boundary**. The platform doesn't care what the agent does — it cares that credentials are hidden, requests are inspected, actions are logged, and spend is capped. Any OpenClaw skill on ClawHub becomes a UseZombie skill with one addition: the trust layer (credential policy, network scope, spend ceiling, AI firewall). The skill registry already exists. The agent code already exists. UseZombie is the missing runtime.

### Trigger types

```
webhook:   — external event (agentmail, slack, github)
cron:      — scheduled (daily briefing, weekly report)
manual:    — operator runs it from CLI/UI
```

### Workspace 1: Lead Management

```
Workspace: "acme-leads"

Agent: lead-collector
  trigger: webhook (agentmail → message.received)
  skills: [agentmail]
  does: parse lead email → auto-reply with invite code → log

Agent: morning-briefing
  trigger: cron (8am daily)
  skills: [agentmail, slack]
  does: read yesterday's leads → slack #founders with summary
```

**What connects them:** The activity stream. Agent 1 writes events (lead received, reply sent). Agent 2 reads them (summarize yesterday). They don't talk to each other — they share a workspace log.

### Workspace 2: PR Review Bot

```
Workspace: "acme-code-review"

Agent: pr-reviewer
  trigger: webhook (github → pull_request.opened)
  skills: [github, git, slack]
  does: clone repo → read diff → post review → slack if critical

Agent: weekly-eng-digest
  trigger: cron (monday 9am)
  skills: [github, slack]
  does: PRs merged last week, avg review time → slack #engineering
```

### Workspace 3: Slack-to-PR (the killer use case)

```
Workspace: "acme-slack-fixes"

Agent: slack-fixer
  trigger: webhook (slack → message, filtered to #bugs channel)
  skills: [slack, git, github]
  does: read bug report → clone repo → find bug → write fix →
        make lint/test/build → open PR → reply in slack thread

Agent: fix-tracker
  trigger: cron (end of day)
  skills: [slack, github]
  does: daily fix report → slack #engineering
```

### Workspace 4: Infrastructure Health (Playbook Agents)

```
Workspace: "acme-infra-health"

Agent: tunnel-watchdog
  trigger: cron (every 5 minutes)
  skills: [cloudflare, slack]
  does: check tunnel status → alert #ops if down → daily uptime summary

Agent: cert-watcher
  trigger: cron (daily 9am)
  skills: [cloudflare, slack]
  does: check SSL cert expiry → alert if <14 days

Agent: deploy-verifier
  trigger: webhook (github → deployment_status)
  skills: [github, cloudflare, slack]
  does: verify health endpoint → check tunnel routing → alert or rollback
```

### The pattern

Every workspace follows the same shape:

```
Workspace (one use case)
│
├── Event-driven agents (webhook triggers)
│   "something happened → do something about it"
│
├── Scheduled agents (cron triggers)
│   "check, summarize, report, follow up"
│
└── Activity stream (shared state)
    agents write events, other agents read them
```

---

## How This Maps to kogo.ai

kogo.ai is "AI that runs your business" — the operator-facing product. UseZombie is the infrastructure underneath it.

```
kogo.ai (operator interface)
  "I want an AI that handles my leads, reviews PRs, and briefs me every morning"
  │
  │  Operator configures workspaces, picks skills, sets budgets
  │  Never touches code. Never sees markdown. Just picks what agents do.
  │
  ▼
usezombie (runtime infrastructure)
  "I run those agents safely — credentials hidden, actions logged, spend capped"
  │
  │  Runs the sandboxes, injects credentials, routes webhooks,
  │  enforces budgets, AI firewall, logs everything, kill switch
  │
  ▼
clawhub (skill registry)
  "Here are the tools agents can use"
  agentmail, slack, github, git, cloudflare, linear, etc.
```

| | kogo.ai | usezombie |
|---|---|---|
| **User** | Business operator, founder, team lead | Developer, platform engineer, agent builder |
| **Interface** | Web dashboard, natural language | CLI + API |
| **Pitch** | "AI that runs your business" | "Runtime that runs your agents safely" |
| **Value prop** | "Set up agents in 5 minutes, no code" | "Agent runtime with credential firewall" |
| **Pricing** | Per-workspace, SaaS subscription | Per-run, infrastructure pricing |
| **Analogy** | Shopify (merchant dashboard) | Shopify backend (payments, inventory, shipping) |

---

## Interfaces

### CLI (ships first)

```bash
zombiectl install <skill>         # install from clawhub
zombiectl config set <key> <val>  # configure
zombiectl up                      # start all agents
zombiectl status                  # what's running, last events, firewall stats
zombiectl logs <agent>            # full activity log
zombiectl chat <agent>            # talk to a running agent
zombiectl stop <agent>            # kill switch
zombiectl credentials add <name>  # add a credential to the store
zombiectl firewall                # show firewall metrics and blocked requests
```

### Web UI (ships with v1)

Dashboard showing your workspace: running agents, recent activity, chat panel, credential store management, webhook status, AI firewall metrics. Hosted at `app.usezombie.com` or self-hosted.

### API

Full REST API for programmatic control. Agent-to-agent orchestration. Upstream planners trigger runs via API, get scorecards and audit trails back.

---

## How UseZombie Makes Money

A "run" is one agent action cycle: receive event → process → respond → log.

| Plan | Always-on agents | Runs/month | Price |
|------|-----------------|------------|-------|
| Free | 1 | 200 | $0 |
| Pro | 5 | 5,000 | $29/mo |
| Team | 20 | 50,000 | $99/mo |
| Scale | Unlimited | 500,000 | $299/mo |

Overage: $0.10 per 100 runs.

The credential firewall, AI firewall metrics, audit log, and chat panel are included in all tiers. They're not upsells — they're the product. Without them, you'd just use Docker.

**Later:**
- **ClawHub marketplace** — third-party skills, revenue share.
- **Compliance add-on** — extended retention, replay, SOC 2 audit trail.
- **Self-hosted license** — for teams that can't send traffic through hosted infra.
- **kogo.ai** — no-code operator layer on top of UseZombie runtime.

---

## What Exists Today (v1)

- `zombied` — Zig daemon. Workspace runtime, process supervisor.
- `zombie-worker` + `zombie-executor` (nullclaw) — agent sandboxes via bubblewrap + landlock.
- `zombiectl` — JS/Bun CLI. Workspace control, run management, agent profiles.
- API: full OpenAPI spec with workspaces, runs, agents, harness, credentials, skill secrets, SSE streaming.
- GitHub App automation (PR creation, branch push, scorecard comment).
- Self-repair gate loop with agent self-fix.
- Agent profiles with trust levels, scoring, and self-tuning proposals.

### What needs building for v2

1. **AI Firewall + Credential proxy** — the core differentiator.
2. **Webhook router** — receive and route webhooks from external services.
3. **Always-on mode** — process supervision, crash restart, cron triggers.
4. **Activity stream** — shared event log per workspace.
5. **Firewall metrics dashboard** — trust score trending.
6. **Multi-agent workspaces** — multiple agents per workspace, each with own harness.
7. **Chat interface** — talk to running agents.
8. **Web UI** — dashboard, activity stream, credential management, firewall metrics.

### What gets deferred

- Multi-agent competition / scored selection
- Score-gated auto-merge
- Firecracker sandbox backend
- Marketplace / third-party skills
- Compliance / SOC 2 reporting
- kogo.ai no-code layer

---

## What Ships First

Two skills. CLI + web interface. That's it.

**Lead Collector:** agentmail webhook → parse email → auto-reply with invite code → log.

**Slack Responder:** slack webhook → classify message → reply or escalate → log.

```bash
zombiectl install lead-collector
zombiectl credentials add agentmail
zombiectl up
```

---

## The Go-To-Market

1. **Ship lead-collector and slack-responder as free ClawHub skills** that work with plain OpenClaw. Get adoption on the skill format.
2. **Show the "what happens when it goes wrong" story** — the $400 invoice, the 3am hallucination, the prompt injection. That's where UseZombie's trust layer sells itself.
3. **Slack-to-PR workspace is the hero demo** — every engineering team has a #bugs channel with stale bugs. An agent that goes from slack message → PR in minutes, with credential isolation, is the "shut up and take my money" moment.
4. **Infrastructure health is the expansion play** — once operators trust UseZombie for code, they trust it for infra monitoring. Same trust layer, different skills.
5. **kogo.ai is the no-code layer** — when UseZombie has enough workspaces and skills proven, kogo.ai wraps it in a UI for non-technical operators.
