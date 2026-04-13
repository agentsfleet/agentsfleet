The 4 Actors (UseZombie edition)

  ┌──────────────────┬──────────────────────────────────────────────────────┬─────────────────────────────────────────────────────────────────────────────┐
  │       ACP        │                      UseZombie                       │                                What they do                                 │
  ├──────────────────┼──────────────────────────────────────────────────────┼─────────────────────────────────────────────────────────────────────────────┤
  │ Buyer            │ Workspace owner (human)                              │ Sets up zombies, grants integration access once, approves high-risk actions │
  ├──────────────────┼──────────────────────────────────────────────────────┼─────────────────────────────────────────────────────────────────────────────┤
  │ Agent            │ Zombie (Lead Collector, Hiring, Ops)                 │ Executes the outcome, declares what integrations it needs                   │
  ├──────────────────┼──────────────────────────────────────────────────────┼─────────────────────────────────────────────────────────────────────────────┤
  │ Seller           │ Integration service (Gmail, Slack, Discord, Grafana) │ Receives the proxied requests                                               │
  ├──────────────────┼──────────────────────────────────────────────────────┼─────────────────────────────────────────────────────────────────────────────┤
  │ Payment Provider │ UseZombie core                                       │ Holds credentials, enforces firewall, injects on proxy, never leaks         │
  └──────────────────┴──────────────────────────────────────────────────────┴─────────────────────────────────────────────────────────────────────────────┘

  ---
  The 3 Zombie Use Cases — Actors + Flow

  1. Lead Collector Zombie

  [AgentMail/Gmail] → email arrives
      → Zombie reads email (UseZombie injects Gmail OAuth)
      → Zombie extracts lead fields
      → Zombie writes to CRM (UseZombie injects CRM key)
      → Zombie sends confirmation (UseZombie injects Slack/email token)
  Human: approves once ("Lead Collector can read my Gmail Inbox, write to HubSpot")
  Human-in-loop: triggered only if lead score > threshold ("Add this lead to paid tier?")

  2. Hiring Agent Zombie

  [Slack] → HR pings #hiring with candidate email
      → Zombie receives Slack event (webhook, UseZombie validates)
      → Zombie reads candidate context (UseZombie injects email/ATS key)
      → Zombie drafts response, posts to Slack thread (UseZombie injects Slack bot token)
  Human: approves once ("Hiring Zombie can read #hiring, post to threads, read Lever/Greenhouse")
  Human-in-loop: triggered for "send offer letter" or "reject candidate" actions

  3. Ops Zombie

  [Grafana] → log stream / alert webhook fires
      → Zombie reads log context (UseZombie injects Grafana API key)
      → Zombie classifies: noise / warning / critical
      → Zombie alerts to Slack or Discord (UseZombie injects bot token)
  Human: approves once ("Ops Zombie can read Grafana logs, post to #alerts")
  Human-in-loop: triggered for "page on-call" or "auto-scale infra" actions

  ---
  The Real Design Question for M9_001

  The original spec frames this as "external agent calls /v1/execute with an API key" — very DevOps-y, very manual. Based on what you described, the better model is:

  Integration Grant Authorization (not API key management)

  SETUP (once, human does this):
    Human creates zombie → declares integration needs:
      {zombie: "hiring-agent", needs: [{service: "slack", scopes: ["channels:read", "chat:write"]}]}
    UseZombie sends human: "Hiring Agent wants Slack access — Approve?"
    Human approves in Slack/dashboard → UseZombie records an Integration Grant
    From now on: Hiring Agent zombie can call Slack without re-approval

  RUNTIME (agent-driven, no human):
    Zombie calls POST /v1/execute {target: "slack.com/api/chat.postMessage", ...}
    UseZombie checks: does this zombie have a valid grant for slack.com? → yes
    UseZombie injects Slack bot token → proxies → returns response
    Agent never sees the token

  HIGH-RISK ACTION (human-in-loop, per spec):
    Zombie calls POST /v1/execute {target: "lever.co/api/offers", action: "send_offer"}
    Firewall matches endpoint rule: requires_approval
    UseZombie DMs workspace owner: "Hiring Agent wants to send offer to Jane Doe — Approve?"
    Human clicks Approve → execution resumes

  ---
  What this means for the spec scope

  ┌────────────────────────┬────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
  │    Original Section    │                                                            Revised Framing                                                             │
  ├────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ 2.0 API Key Management │ Integration Grant Management — zombie declares needs, human approves, UseZombie issues a scoped grant (not a raw key the user manages) │
  ├────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ 1.0 Execute Endpoint   │ Same — /v1/execute is still the proxy endpoint                                                                                         │
  ├────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ 3.0 Execute Pipeline   │ Same — firewall → grant check → cred inject → proxy → scan                                                                             │
  ├────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┤
  │ 4.0 CLI Commands       │ zombiectl grant list, zombiectl grant revoke {id} (not api-key)                                                                        │
  └────────────────────────┴────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘

  ---
  Integration actors to design for

  You said keep it small: AgentMail/Gmail, Slack, Discord

  ┌───────────────────┬────────────────────────────────────────────────┬─────────────────────────────────┐
  │      Service      │               Zombie uses it for               │         Credential type         │
  ├───────────────────┼────────────────────────────────────────────────┼─────────────────────────────────┤
  │ AgentMail / Gmail │ Receive emails (Lead Collector, Hiring)        │ OAuth token / AgentMail API key │
  ├───────────────────┼────────────────────────────────────────────────┼─────────────────────────────────┤
  │ Slack             │ Receive events + post messages (all 3 zombies) │ Bot token                       │
  ├───────────────────┼────────────────────────────────────────────────┼─────────────────────────────────┤
  │ Discord           │ Post alerts (Ops Zombie)                       │ Bot token                       │
  ├───────────────────┼────────────────────────────────────────────────┼─────────────────────────────────┤
  │ Grafana           │ Pull log streams / receive alerts (Ops)        │ API key                         │
  └───────────────────┴────────────────────────────────────────────────┴─────────────────────────────────┘

  ---
  Before I rewrite the spec and resume implementation — two questions:

  1. Who initiates the integration request? The human sets up the zombie and says "this zombie needs Slack" (dashboard/CLI), or the zombie itself sends a capability request the first time it tries to
  call an integration?
  2. Grant granularity: Is one grant per service (slack), per channel (slack:#hiring), or per action (slack:chat.postMessage)? The Stripe/ACP model uses scope arrays — do you want the same?
