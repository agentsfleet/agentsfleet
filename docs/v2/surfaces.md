# UseZombie v2 — Product Surfaces

Date: Apr 07, 2026: 10:00 AM
Status: Draft — content source for v2 website, app, CLI, and API surfaces

---

## Three surfaces, one runtime

| Surface | URL | User | Purpose |
|---------|-----|------|---------|
| **Marketing site** | `usezombie.com` | Everyone | Why UseZombie exists. Flat tire story. Walled, watched, killable. |
| **App** | `app.usezombie.com` | Operators (human-facing) | Configure workspaces, manage agents, view firewall metrics, chat with agents, kill switch. |
| **CLI + API** | `zombiectl` / `api.usezombie.sh` | Developers, agents, CI | Programmatic control. Install skills, manage credentials, start/stop agents, query audit trail. |

---

## Surface 1: Marketing Site (`usezombie.com`)

### Hero

```yaml
badge: "Heroku for agents, but the agent never sees your keys"
line1: "Run your agents 24/7."
line2: "Walled, watched, killable."
kicker: >
  UseZombie is a zero-trust runtime for always-on AI agents.
  Credentials hidden at the firewall. Every action logged.
  Spend ceiling enforced. Kill switch included.
  You bring the agent — we handle the walls.
cta_primary: "Get started free"
cta_secondary: "See how it works"
```

### The Problem (above the fold)

```yaml
headline: "You built an agent. Now what?"
problems:
  - "You pasted your Slack token into an env var. Your agent can see it."
  - "You wrapped it in Docker. It ran up $400 at 3am."
  - "It posted garbage in #general. You can't explain why."
  - "You want it to do more. But you don't trust it unsupervised."
pivot: >
  There are great tools for building agents. There's nothing
  purpose-built for running them safely, always on, with
  credentials hidden and every action logged.
  That's UseZombie.
```

### How It Works (3 steps)

```yaml
steps:
  - title: "Install a zombie"
    description: >
      Write (or clone a sample) — a zombie is two markdown files,
      SKILL.md and TRIGGER.md, that declare what the agent does and
      what services it can reach. One command registers it and it's live.
    command: "zombiectl install --from samples/homelab"

  - title: "Add credentials (hidden from the agent)"
    description: >
      Store your API keys in UseZombie's encrypted vault. The agent
      never sees them. When the agent makes an HTTP request, the
      firewall outside the sandbox injects the credential per-request
      and strips it from the response.
    command: "zombiectl credentials add agentmail"

  - title: "Watch and steer"
    description: >
      Your zombie runs 24/7 in a sandboxed process. Webhooks are
      wired automatically. Every action is logged. If it goes wrong,
      the spend ceiling stops it and the kill switch is one command away.
    command: "zombiectl status"
```

### Features (6 cards)

```yaml
features:
  - number: "01"
    title: "Credentials hidden at the firewall"
    description: >
      Agents never see tokens. The AI firewall intercepts outbound
      requests, injects credentials per-request from an encrypted vault
      outside the sandbox, and strips any token echo from responses.
      Prompt injection can't exfiltrate what doesn't exist in the sandbox.

  - number: "02"
    title: "AI firewall with policy enforcement"
    description: >
      Every outbound request is inspected: domain allow-list check,
      API endpoint policy, prompt injection pattern detection, content
      scanning. The firewall blocks what the agent shouldn't do —
      even if the agent has been compromised.

  - number: "03"
    title: "Spend ceiling, not surprise invoices"
    description: >
      Per-run token budgets, wall time limits, and kill-on-exceed.
      One hallucination loop doesn't become a $400 invoice at 3am.
      You set the ceiling before the agent runs.

  - number: "04"
    title: "Full audit trail"
    description: >
      Every request, webhook, credential injection, and policy decision
      is timestamped and replayable. When someone asks "what did the
      agent do at 3am?" you have the answer in seconds.

  - number: "05"
    title: "Always-on with kill switch"
    description: >
      Agents run continuously with process supervision and crash restart.
      Stop any agent mid-action from the CLI or web UI. One command.

  - number: "06"
    title: "Webhooks wired, not hacked"
    description: >
      Receive events from agentmail, Slack, GitHub, Cloudflare, and
      more. No ngrok. No custom servers. UseZombie registers webhooks
      on skill install and routes events to the right agent.
```

### Workspace Examples

```yaml
headline: "One platform. Any use case."
subline: >
  A workspace is a group of agents that share an activity stream.
  Event-driven agents respond to webhooks. Scheduled agents summarize
  and report. The platform doesn't care what the agent does — it
  cares that credentials are hidden, requests are inspected, actions
  are logged, and spend is capped.

examples:
  - name: "Lead Management"
    agents: ["lead-collector (webhook)", "morning-briefing (cron)"]
    skills: [agentmail, slack]
    story: >
      Inbound emails auto-replied with invite codes. Founder gets
      a daily Slack summary of yesterday's leads.

  - name: "PR Review Bot"
    agents: ["pr-reviewer (webhook)", "weekly-digest (cron)"]
    skills: [github, git, slack]
    story: >
      New PRs get automated security and style review. Engineering
      gets a weekly digest of merged PRs and review times.

  - name: "Slack-to-PR"
    agents: ["slack-fixer (webhook)", "fix-tracker (cron)"]
    skills: [slack, git, github]
    story: >
      Bug report in #bugs → agent finds the bug, writes a fix,
      opens a PR, replies in the Slack thread with the link.

  - name: "Infrastructure Health"
    agents: ["tunnel-watchdog (cron)", "cert-watcher (cron)", "deploy-verifier (webhook)"]
    skills: [cloudflare, github, slack]
    story: >
      Cloudflare tunnel monitored every 5 minutes. SSL certs checked
      daily. Deploy health verified on every push.

  - name: "Your workspace"
    agents: ["your-agent"]
    skills: ["any ClawHub skill"]
    story: >
      Install skills, add credentials, write instructions in markdown,
      start the agent. The platform handles the rest.

open_ended: >
  These are sample use cases, not the product boundary. Any OpenClaw
  skill on ClawHub becomes a UseZombie skill with one addition —
  the trust layer. The skill registry already exists. The agent code
  already exists. UseZombie is the runtime.
```

### AI Firewall Dashboard (visual)

```yaml
headline: "Proof your agents are behaving."
subline: >
  Not just logs — actionable security metrics. See what was blocked,
  what was flagged, and how your agents' trust scores trend over time.

metrics:
  - "Requests proxied"
  - "Credentials injected"
  - "Prompt injections blocked"
  - "Policy violations caught"
  - "Hallucination kills"
  - "Domains blocked"
  - "Budget kills"
  - "Trust score (7-day trend)"
```

### Target Users

```yaml
users:
  - name: "Solo builder / founder"
    hook: "Run your agent 24/7 without worrying about leaked tokens."
    detail: >
      Install a skill, add your credentials to the vault, start the
      agent. UseZombie handles webhooks, credential isolation, audit,
      and spend control. You check the dashboard once a day.

  - name: "Engineering team"
    hook: "Let agents touch Slack, GitHub, and prod APIs — safely."
    detail: >
      Each agent gets scoped credentials and network policy. The AI
      firewall blocks what agents shouldn't do. Full audit trail for
      compliance. Kill switch for emergencies.

  - name: "Agent-to-agent (API)"
    hook: "Your orchestrator triggers agents via API. UseZombie runs them."
    detail: >
      UseZombie is the execution layer. Upstream agents submit tasks,
      get audit trails back. One stable API contract. Credential
      isolation between agents.

  - name: "OpenClaw / ClawHub skill author"
    hook: "Your skill runs safely on UseZombie with zero extra code."
    detail: >
      Write a SKILL.md with frontmatter. Declare what services you
      need. UseZombie handles credential injection, network policy,
      and observability automatically.
```

### Pricing

```yaml
plans:
  - name: "Free"
    price: "$0"
    features:
      - "1 always-on agent"
      - "200 runs/month"
      - "AI firewall included"
      - "Full audit trail"
      - "Community support"

  - name: "Pro"
    price: "$29/mo"
    features:
      - "5 always-on agents"
      - "5,000 runs/month"
      - "Priority execution"
      - "Firewall metrics dashboard"
      - "Email support"

  - name: "Team"
    price: "$99/mo"
    features:
      - "20 always-on agents"
      - "50,000 runs/month"
      - "Multiple workspaces"
      - "Extended audit retention"
      - "Slack support"

  - name: "Scale"
    price: "$299/mo"
    features:
      - "Unlimited agents"
      - "500,000 runs/month"
      - "SSO + RBAC"
      - "Compliance export"
      - "Dedicated support"

overage: "$0.10 per 100 runs"

note: >
  AI firewall, credential isolation, audit trail, and kill switch
  are included in ALL tiers. They're the product, not upsells.
```

### Messaging Guardrails

```yaml
do_say:
  - "zero-trust"
  - "credential isolation"
  - "AI firewall"
  - "always-on"
  - "audit trail"
  - "spend ceiling"
  - "kill switch"
  - "your agent, your rules"

do_not_say:
  - "AI writes your code" — UseZombie runs agents, it doesn't build them
  - "fully autonomous" — human has kill switch and approval gates
  - "we store your credentials securely" — too weak; say "agents never see credentials"
  - "Zapier for agents" — we run agent code, not no-code automations
  - "single binary"
```

---

## Surface 2: App (`app.usezombie.com`)

Human-facing dashboard for operators. No code required.

### Pages

```yaml
pages:
  - name: "Dashboard"
    purpose: "Overview: running agents, recent activity, firewall stats, spend"
    components:
      - agent status cards (running / stopped / crashed)
      - activity stream (last 50 events)
      - firewall metrics summary
      - spend tracker (budget used / remaining)
      - kill switch buttons

  - name: "Workspace"
    purpose: "Configure a workspace: agents, skills, triggers, credentials"
    components:
      - agent list with harness config
      - skill picker (from ClawHub)
      - credential vault (add/remove, never shows values)
      - webhook status (registered, last received)
      - network policy viewer

  - name: "Agent Detail"
    purpose: "Deep view of one agent: logs, chat, firewall events"
    components:
      - activity log (full audit trail, filterable)
      - chat panel (talk to running agent)
      - firewall events (blocked requests, policy violations)
      - trust score chart (7-day trend)
      - spend breakdown (tokens, wall time, cost)

  - name: "Firewall"
    purpose: "AI firewall metrics and blocked request details"
    components:
      - requests proxied / credentials injected
      - prompt injections blocked (with details)
      - policy violations (with request body preview)
      - hallucination kills
      - domain block list with hit counts
      - trust score trending

  - name: "Credentials"
    purpose: "Manage the credential vault"
    components:
      - list credentials (name, scope, last used — never the value)
      - add credential (encrypted at rest)
      - delete credential
      - credential usage log (which agent, which request, when)

  - name: "Settings"
    purpose: "Workspace settings, billing, team"
    components:
      - billing plan and usage
      - team members and roles
      - API keys for programmatic access
      - webhook endpoint configuration
```

---

## Surface 3: CLI + API (`zombiectl` / `api.usezombie.sh`)

Developer and agent-facing. Programmatic control of everything in the app.

### CLI Commands

```yaml
commands:
  # Zombie management
  - cmd: "zombiectl install --from <path>"
    does: "Register the zombie at <path>/SKILL.md + <path>/TRIGGER.md; server activates it atomically"
  - cmd: "zombiectl list"
    does: "List installed zombies and their status"

  # Zombie lifecycle
  - cmd: "zombiectl kill <zombie>"
    does: "Kill switch — stop zombie mid-action"
  - cmd: "zombiectl status"
    does: "Running zombies, last events, firewall stats, spend"
  - cmd: "zombiectl logs --zombie <id>"
    does: "Full activity log with firewall events"

  # Credentials
  - cmd: "zombiectl credentials add <name>"
    does: "Add credential to encrypted vault (prompted, never in args)"
  - cmd: "zombiectl credentials list"
    does: "List credentials (name + scope, never values)"
  - cmd: "zombiectl credentials remove <name>"
    does: "Remove credential from vault"

  # Chat
  - cmd: "zombiectl chat <agent>"
    does: "Interactive chat with running agent"

  # Firewall
  - cmd: "zombiectl firewall"
    does: "Show firewall metrics: blocked, injected, trust score"
  - cmd: "zombiectl firewall blocked"
    does: "Show blocked requests with details"

  # Workspace
  - cmd: "zombiectl workspace create <name>"
    does: "Create a new workspace"
  - cmd: "zombiectl workspace list"
    does: "List workspaces with agent counts"
```

### API Endpoints (v2 additions to existing OpenAPI)

```yaml
new_endpoints:
  # Webhooks
  - "POST /v1/workspaces/{ws}/webhooks"
    does: "Register webhook endpoint with external service"
  - "GET /v1/workspaces/{ws}/webhooks"
    does: "List registered webhooks and status"
  - "POST /v1/hooks/{ws}/{skill}"
    does: "Incoming webhook receiver (public, signature-verified)"

  # Agent lifecycle
  - "POST /v1/workspaces/{ws}/agents/{agent}:start"
    does: "Start agent process"
  - "POST /v1/workspaces/{ws}/agents/{agent}:stop"
    does: "Kill switch"
  - "GET /v1/workspaces/{ws}/agents/{agent}/status"
    does: "Running, stopped, crashed (with restart count)"

  # Chat
  - "POST /v1/workspaces/{ws}/agents/{agent}/chat"
    does: "Send message to running agent, get response"

  # Activity stream
  - "GET /v1/workspaces/{ws}/activity"
    does: "Shared event log, filterable by agent, type, time"

  # Firewall
  - "GET /v1/workspaces/{ws}/firewall/metrics"
    does: "Firewall stats: blocked, injected, trust score"
  - "GET /v1/workspaces/{ws}/firewall/events"
    does: "Blocked requests, policy violations, with details"

  # Existing endpoints that carry over
  existing:
    - "/v1/workspaces — create, pause, sync"
    - "/v1/runs — create, list, get, retry, cancel, stream, replay"
    - "/v1/agents — profiles, scores, proposals"
    - "/v1/workspaces/{ws}/harness — source, compile, activate, active"
    - "/v1/workspaces/{ws}/credentials/llm — BYOK"
    - "/v1/workspaces/{ws}/skills/{skill}/secrets — per-skill secrets"
```

---

## v1 → v2 transition notes

### What stays from v1 website content

- Pricing page structure (updated tiers and features)
- BYOK messaging (still relevant — agents bring their own LLM keys)
- Sandbox execution messaging (still the foundation)

### What changes

| v1 message | v2 message |
|-----------|-----------|
| "Submit a spec. Get a validated PR." | "Run your agents 24/7. Walled, watched, killable." |
| Spec-driven workflow | Skill-driven, event-driven workflow |
| Self-repair gate loop | AI firewall + credential isolation |
| Scored PRs with evidence | Full audit trail with firewall metrics |
| "Connect GitHub, automate PRs" | "Install a skill, add credentials, start the agent" |
| Agent profiles (fast-shipper, test-heavy) | Workspace templates (leads, PR review, slack-to-PR, infra) |

### What's new

- AI firewall as a headline feature (not just sandboxing — active inspection)
- Firewall metrics dashboard (proof agents are behaving)
- Workspace examples (not just one pipeline — any use case)
- ClawHub skill registry integration
- kogo.ai as the no-code operator layer (future)
- "The agent never sees a credential" as the core differentiator
