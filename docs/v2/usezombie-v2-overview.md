# UseZombie v2 — Overview

**Run your agents 24/7. We handle the credentials, webhooks, and walls.**

**One sentence:** UseZombie runs your AI agent 24/7 without giving it your passwords.

**Positioning:** Heroku for agents, but the agent never sees your keys.

**Tagline:** Run your agents 24/7. Walled, watched, killable.

**See also:**
- [AI Firewall](usezombie-v2-firewall.md) — credential proxy, policy enforcement, metrics
- [Runtime & Workspaces](usezombie-v2-runtime.md) — skills, multi-agent workflows, interfaces, pricing, GTM
- [Product Surfaces](surfaces.md) — content map for marketing site, app, CLI+API

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
