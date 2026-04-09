# UseZombie v2 — Overview

```
$ zombiectl install lead-collector
$ zombiectl up

Your agent is live. It's replying to emails right now.
```

**Credentials hidden. Every action logged. Big moves approved in Slack.**

**The unit:** A **Zombie** — a pre-built, configurable agent workflow that does one job, runs 24/7, and never needs babysitting.

**Internal positioning (not customer-facing):** You'd run your agent 24/7 if you trusted it. UseZombie is the trust layer.

**See also:**
- [AI Firewall](usezombie-v2-firewall.md) — credential proxy, policy enforcement, approval gates
- [Runtime & Workspaces](usezombie-v2-runtime.md) — Zombies, tools, workspaces, interfaces, pricing, GTM
- [Product Surfaces](surfaces.md) — content map for marketing site, app, CLI+API

---

## The Problem You Have Today

You built an agent. It works when you run it on your laptop. Then you try to make it run continuously:

- You paste your GitHub token / Slack token / API key into an env var.
- You wrap it in Docker and run it on a VPS.
- You realize the agent can see your tokens in plain text.
- You realize if it gets prompt-injected, it can exfiltrate those tokens.
- You realize you have no log of what it actually did at 3am.
- You realize you have no way to stop it mid-action.
- You realize you have no ceiling on what it can spend or charge.

So you don't run it. Or you babysit it. Or you pay for a virtual credit card (Firepass, $7/mo) just to give it a scoped way to spend money, because you won't give it your real card.

**You already trust what the agent can do. You don't trust it with real, unsupervised access.**

That's the flat tire. UseZombie fixes that.

---

## What UseZombie Is

A runtime for always-on agents — with a trust layer built in.

You install a **Zombie** (or configure your own). You tell it which tools it needs: email, Slack, GitHub, git, Cloudflare. UseZombie runs it with:

- **Credentials hidden** — the agent never sees a token. The sandbox boundary holds credentials outside the agent process. The firewall injects them per-request.
- **Approval gates** — before a high-stakes action (push to main, charge a new customer, send to a list), the Zombie pauses and sends a Slack message: [Approve] [Deny]. You tap. It proceeds or stops. Like 3D Secure for card payments — fires rarely, on anomalies only.
- **Full audit trail** — every action, every credential use, every policy decision, timestamped and replayable. When someone asks "what did it do at 3am?" you have the answer in seconds.
- **Spend ceiling** — per-Zombie token budgets, wall time limits, kill-on-exceed. A hallucination loop that fires 847 requests is caught by pattern detection and killed before it becomes an invoice.
- **Kill switch** — stop any Zombie mid-action from Slack or the CLI.
- **Tools as attachments** — git, GitHub, Slack, agentmail are tools a Zombie attaches when it needs them. The sandbox doesn't know about git. The Zombie does. Clean separation.

---

## Zombies

A Zombie is a pre-built, configurable agent workflow. You configure it, you don't code it. Install one from ClawHub, add credentials, set a budget, and it runs.

```
Zombie: lead-collector
  tools: [agentmail]
  trigger: webhook (email received)
  does: parse email → reply with invite code → log to activity stream

Zombie: slack-bug-fixer
  tools: [slack, git, github]
  trigger: webhook (message in #bugs)
  does: read bug report → clone repo → find bug → fix →
        make lint/test → open PR → reply in slack thread
  approval gate: push to main → Slack message [Approve] [Deny]

Zombie: pr-reviewer
  tools: [github, slack]
  trigger: webhook (PR opened)
  does: read diff → post review → alert #eng if critical

Zombie: ops-watchdog
  tools: [cloudflare, slack]
  trigger: cron (every 5 min)
  does: check tunnel status → alert #ops if down

Zombie: security-gate
  tools: []
  does: receive action → check policy → gate if threshold →
        log → execute or deny → repeat
```

The Security Zombie is just another Zombie. It has the same structure as all the others — trigger, tools, loop. Not a separate API product. A configurable workflow.

---

## Entry Point: Slack Plugin

The fastest path to a running Zombie:

```
1. Add UseZombie to your Slack workspace
2. Bot asks: "What should your Zombie do?"
3. Pick: "Fix bugs from #bugs"
4. Add GitHub credentials (stored in vault, never shown to agent)
5. Zombie is live in 5 minutes
6. Approval gates fire in the same Slack channel — [Approve] [Deny] buttons
```

No landing page required to start. No credit card until you're sure it works. The approval gate fires in Slack — where the team already is.

---

## The Flat Tire Test

Three scenarios a developer pays to fix *today*:

### 1. "My agent charged a customer 12 times"

Agent processes orders. Upstream API times out on the charge. Agent retries — instructions say "retry on failure." The first charge went through but the response didn't arrive. Agent retries 12 more times. Each charge looks legitimate to Stripe individually.

**UseZombie answer:** Pattern detection catches "12 charges to the same customer in 90 seconds, 11x above baseline" and kills the Zombie before charge 3. Operator gets a Slack alert.

### 2. "My agent did something at 3am and I can't explain what or why"

This isn't observability as a feature — it's observability as *liability cover*. The audit trail is also how you expand agent permissions over time: watch the log for a week, see clean behavior, then let the Zombie do more. Without the trail, you never have the evidence to trust it with more authority.

**UseZombie answer:** Every action logged, timestamped, queryable. Full explanation for any moment the agent ran.

### 3. "I want to let it do more, but I can't give it unsupervised access"

You already use a virtual credit card for your agent because you won't give it your real card. That's one credential, one dimension. You need this for GitHub, Slack, email, Cloudflare, and every other API your agent touches.

**UseZombie answer:** The Zombie declares which tools it needs. UseZombie injects credentials at the sandbox boundary — the agent never sees them. Network deny-by-default blocks everything else. You trust the boundary, not the agent.

---

## Competitive Landscape (April 2026)

**Nobody does all three** — credential isolation + approval gates + always-on Zombies with real tools.

| Product | Credential isolation | Approval gate | Always-on | Tools as attachments |
|---------|---------------------|---------------|-----------|---------------------|
| **UseZombie** | YES — sandbox boundary | YES — 3D Secure model | YES | YES — per-Zombie |
| **Composio** | YES (managed OAuth) | No | No (auth layer only) | No |
| **LangGraph Platform** | No (env vars) | No | YES | No |
| **Modal** | No (env vars) | No | YES | No |
| **E2B** | No (sandbox only) | No | No (ephemeral) | No |
| **Cursor Automations** | No | No | YES (Cursor cloud) | No |

**The companies solving "AI firewall"** (Robust Intelligence, Lakera, Protect AI, Guardrails.ai)
are solving **model output safety** — is this LLM response safe? That's a different threat model.
UseZombie solves **agent action authorization** — is this action against Stripe/GitHub/Slack allowed?
Different problem. Different buyer. Not the same market.

**Why not "just use Composio + LangGraph + Datadog"?**
Three products, three bills, three failure modes, no approval gate, no coordinated audit trail.
UseZombie is the integrated stack. One Slack plugin. One credential vault. One audit trail.

---

## Architecture

**One architecture: UseZombie sandbox.**
bwrap + landlock isolation. Credentials stored outside the process boundary.
Network deny-by-default. All Zombies run in the sandbox. This is what makes the
security guarantee real — not policy, enforcement.

**For external agents (LangGraph, CrewAI, OpenAI SDK, etc.):**
UseZombie exposes an API endpoint. External agents call it to execute credentialed actions.
UseZombie checks policy, fires approval gate if needed, injects credentials, makes the call,
returns the response. The agent never gets the credential. This IS the Security Zombie —
a packaged workspace that exposes an endpoint. Same architecture as every other Zombie.

```
External agent → POST api.usezombie.com/execute
                 { target: "api.stripe.com/v1/charges", body: {...} }
              ← UseZombie checks policy, approves, injects cred, returns response
```

No proxy. No sidecar. No TLS interception. Just an API.

**Tools are attachments, not sandbox-core:**
v1 had git glued into the executor. v2 rule: the sandbox is tool-agnostic.
Each Zombie declares the tools it needs. The sandbox enforces the security boundary
regardless of what tools are attached. Git is a tool. So is Slack. So is the payment API.

---

## What Exists Today (from v1)

The v2 sandbox is not being built from scratch. v1 shipped:
- bwrap + landlock sandbox executor
- Credential injection at the sandbox boundary
- Kill switch / interrupt (M21_001)
- Spend ceiling and budget tracking (M17_001)
- Worker fleet and process supervision
- zombiectl CLI, full REST API, SSE streaming
- Billing and plan entitlements

**What needs building for v2:**
1. Approval gate (policy engine + Slack notification + resume/kill)
2. Webhook router (receive and route events to the right Zombie)
3. Always-on mode (crash restart, cron triggers)
4. Activity stream (shared event log per workspace)
5. Zombie configuration format (tool attachments, trigger definition)
6. Slack plugin (entry point, approval gate UX)
7. Web dashboard (Zombie status, audit trail, approval history)

**What ships first:**
Lead Zombie (agentmail + approval gate + audit trail) — this week.
Slack Bug Fixer Zombie (slack + git + github + approval gate) — hero demo.
