# Prompt: Generate UseZombie v2 Framework Spec and Scaffolding

You are an experienced systems engineer helping me write the technical
specification and initial scaffolding for UseZombie v2. Everything below
is the context you need. Read it all before you start writing. Then
produce the deliverables listed at the end.

---

## Who I am

I am Kishore, founder of UseZombie (usezombie.com,
github.com/usezombie/usezombie, docs.usezombie.com). Background: Senior
Principal SRE + PM at E2E Networks (Indian GPU cloud, NVIDIA partner).
Built GPU Kubernetes platforms, DRBD upgrade release management across
43 storage nodes, HashiCorp Vault secrets SOPs, marketplace PRDs with
Trivy security scanning, logging architectures on Vector/NATS/ClickHouse.
Prior ventures: megam.io, getattune.com. Based in Delhi, IST.

I am an indie hacker shipping this solo. I want developer-led distribution,
not enterprise sales. I've already built parts of this in v1 — the
runtime (zombied, zombie-worker, zombie-executor/nullclaw in bubblewrap
+ landlock), the CLI (zombiectl, JS/Bun), and an earlier sample called
lead-collector. I am now rewriting as v2 with a sharper thesis.

---

## What UseZombie v2 is

UseZombie is an open-source framework for building AI agents that
operate against real infrastructure — real Kubernetes clusters, real
servers, real codebases, real cloud accounts — without handing the agent
a credential it could hallucinate, log, or leak.

One-line pitch:

    An open-source agent runtime that lets developers ship agents
    which touch real infrastructure safely — starting with homelab
    and side-project devs, then production ops teams.

The framework provides opinionated end-to-end plumbing that every
infrastructure-touching agent needs:

- Sandboxed execution
- Credential vault with boundary-layer injection (agent never holds the
  real secret, only a placeholder)
- Typed skill registry (tools are declared in markdown specs)
- Reasoning loop that selects skills, plans, invokes, observes, iterates
- Per-action approval gates
- Audit log of every decision and tool call
- Kill switch

Developers write a single markdown skill spec plus a prompt. The framework
handles the rest.

---

## What it is NOT

- Not a general-purpose agent framework (that's LangGraph / OpenAI Agents SDK)
- Not a sandbox-as-a-service (that's E2B / Daytona / Modal / Microsandbox)
- Not an LLM gateway (that's Portkey / Bifrost / LiteLLM)
- Not an observability tool (that's Langfuse / Braintrust / Helicone)
- Not an agent auth product (that's Auth0 / Stytch / Nango / Scalekit)

It composes these primitives opinionatedly for one use case: agents
that touch real infrastructure.

---

## The landscape we compete with (honest assessment)

**Sandboxes** (crowded, we do not compete here):
E2B, Daytona, Modal, Sprites (Fly.io), Microsandbox (YC X26, Apache-2.0,
5000+ stars, uses libkrun + placeholder-based secret injection — this is
the closest competitor to our secret-firewall story, and they shipped first).

**Coding agents** (dominated, we are not a coding agent):
Devin, Cursor Cloud Agents, Claude Code, OpenAI Codex, GitHub Copilot agent.

**Agent frameworks** (consolidating around model vendors):
LangGraph, OpenAI Agents SDK (which now has native sandbox integrations
with E2B/Daytona/Modal/Blaxel/Cloudflare/Runloop/Vercel), Google ADK,
Anthropic Claude Agent SDK, CrewAI, AutoGen, Mastra, Pydantic AI.

**Agent observability** (saturated):
Langfuse, Braintrust, Maxim, Galileo, LangSmith, Portkey, Helicone.

**Agent auth** (crowded, enterprise-leaning):
Auth0 for AI, Stytch, Scalekit, Nango, LoginRadius, WorkOS.

Our differentiated wedge is the opinionated combination, targeted at a
specific underserved market: **developers who want to give agents real
access to infrastructure they personally care about.**

---

## The vertical we are launching with

**Homelab Zombie.** A conversational SRE for self-hosted infrastructure.

Target user: a developer who runs k3s / Docker / Proxmox in their home,
cares about Jellyfin / Immich / Paperless / Home Assistant, reads HN,
participates in r/selfhosted and r/homelab, and would love an AI that
can investigate their cluster at 11pm without being handed cluster-admin.

v0.1 is read-only: kubectl get/describe/logs/top/events, docker
ps/logs/inspect, SSH for basic system checks. No writes. No remediation.
Just diagnosis. v0.2 adds writes behind per-verb approval gates.

Homelab Zombie is *a sample built on UseZombie* — not the product. The
product is the framework. Homelab Zombie is our first demo and
distribution channel.

Other planned samples (in `samples/` folder):

- **side-project-resurrector** — resurrects dormant GitHub repos, opens a PR
- **migration-zombie** — mechanical code migrations (Jest→Vitest,
  Express→Hono, Node major bumps) via playbooks
- **homebox-audit** — quarterly security + freshness audit of a homelab stack

We are explicitly NOT shipping:

- lead-collector (was in v1, removed — it's a flat-tyre GTM zombie that
  competes with Clay/11x without a real differentiator)
- Anything that requires enterprise sales to land

---

## Architecture (what I've already decided)

### Components

1. **zombied** — the control plane daemon. Zig. Runs in our cloud (hosted)
   or on the customer's infrastructure (self-hosted). Coordinates runs,
   holds run metadata, emits audit events, serves the web dashboard API.

2. **zombie-worker** — the execution agent. Runs in the customer's
   network. Polls zombied for tasks. Holds credentials locally (in vault).
   Executes skill invocations. Streams reasoning + tool-call results
   back to zombied. Ships as a Docker image. The customer pulls + runs:

       docker run -d --name zombie-worker \
         -e ZOMBIE_TOKEN=... usezombie/worker:latest

3. **nullclaw / zombie-executor** — the sandbox that skills run in,
   inside the worker. bubblewrap + landlock for v2.0. Consider libkrun
   (same as Microsandbox) as a future option if bubblewrap hits real
   isolation limits.

4. **zombiectl** — the user-facing CLI. JS/Bun. Subcommands: auth login,
   install <skill>, worker list, worker token, cred add/list/revoke,
   log show, kill, replay.

5. **zombie** — the conversational agent binary. User types `zombie` to
   start a session with a loaded skill bundle. Maintains context across
   turns. `--once "..."` for one-shot scripting.

6. **clawhub** — the skill registry. A catalog of published skills.
   Analogous to MCP servers / npm for agent tools. Hosts our first-party
   skills (kubectl-readonly, docker-readonly, etc.) and eventually
   community-contributed skills.

### The credential firewall

This is the load-bearing differentiator. The pattern (acknowledged: same
family as Microsandbox's placeholder-based secret injection):

1. User runs `zombiectl cred add kube --name home-k3s --file ~/.kube/config`
2. Kubeconfig is encrypted and stored on the worker in its vault. Never
   transits through the control plane.
3. When a skill needs the credential, the worker generates a random UUID
   placeholder. The sandbox (and the LLM driving it) sees the placeholder.
   The real credential stays in the vault.
4. When the sandboxed process (e.g. `kubectl`) makes an HTTPS call, an
   in-worker proxy catches the call at the network boundary. The proxy
   looks up the placeholder, swaps it for the real credential, originates
   the real request, captures the response, streams it back into the
   sandbox.
5. The LLM context can repeat the placeholder all it wants. The placeholder
   alone does nothing. The real credential never enters the context.

For Kubernetes specifically, this means mTLS re-origination — the proxy
terminates the kubectl-side TLS connection (with a placeholder cert in the
ephemeral kubeconfig) and re-originates to the real API server using the
real client cert from vault. For bearer-token APIs (GitHub, OpenAI, Slack),
simpler HTTP-header injection works.

### Skills

A skill is a markdown file with YAML frontmatter declaring:

- Name, version, description, tags
- Required credentials (types)
- Policy: allowed verbs, denied verbs, auto-approve vs. require-approval
- Worker placement requirements (customer-network for homelab,
  cloud-OK for migrations/resurrector)

See `samples/skills/kubectl-readonly/README.md` for the canonical example
(already written, available in this repo).

Skills compose: a zombie spec (e.g. Homelab Zombie) declares a list of
skills it needs, plus its prompt / reasoning strategy. The zombie at
runtime has access only to the skills in its bundle.

### Audit log

Every tool call, every credential use, every reasoning step is logged:

    {
      "run_id": "zmb_01j9x7y...",
      "timestamp": "2026-04-20T11:47:32Z",
      "kind": "tool_call",
      "skill": "kubectl-readonly",
      "command": "kubectl get pods -n media",
      "verb": "get",
      "resource": "pods",
      "namespace": "media",
      "duration_ms": 142,
      "result": "ok",
      "output_lines": 8,
      "approved_by": "auto"
    }

User-visible via `zombiectl log show <run_id>` and the web dashboard.

### Approval gates (v0.2)

For any tool call that crosses the `require_approval: true` policy line,
the worker pauses the sandbox and publishes an approval request to the
control plane. The user approves via:

- Terminal prompt (if they have `zombie` running interactively)
- Web dashboard
- Slack notification with approve/deny button (v0.2.x)
- Mobile push (v0.3+)

On approval, the worker resumes the sandbox. On deny, the tool call
returns an error; the agent reasons about it and tries something else.

---

## Design principles

1. **The agent is a conversation, not a command tree.** `zombie` starts
   a REPL-like session. User talks. Zombie reasons in a loop. Follow-up
   questions maintain context. One-shot is supported via `--once` but
   is the secondary mode.

2. **Start read-only. Add writes behind approval gates.** Every v0.1
   skill is read-only. Every write skill requires explicit per-action
   approval in v0.2.

3. **Credentials never enter the agent's context.** Placeholder-based
   injection at the network boundary is load-bearing.

4. **The worker runs in the customer's network.** For homelab use, this
   is the customer's Tailscale / home LAN. For production ops, it's
   their VPC. Cloud-only workers are a v1.x convenience for
   non-production use cases (side-project-resurrector can run in our
   cloud because the customer doesn't care — the repo is public or they
   give us a scoped GitHub PAT).

5. **Skills are markdown, not code.** The spec is declarative (policy,
   required credentials, placement). The implementation is a small Zig
   module registered with the worker. For most tools, there's exactly
   one generic implementation (shell allowlist for CLIs like kubectl,
   docker; HTTP-header injection for bearer-token APIs). New skills
   typically are new *policies over the same implementation*.

6. **Open source core (Apache-2.0). Hosted control plane is our product.**
   Worker is self-hostable. Users who want to run the whole stack
   themselves can. Users who want us to handle the control plane pay.

7. **No telemetry by default.** Opt-in analytics only. Homelab devs are
   allergic to phone-home.

---

## Pricing (current thinking, not final)

- **Free:** 20 zombie conversations/month, 1 worker, single-user
- **Hobby ($9/mo):** 200 conversations/month, 1 worker, unlimited skills
- **Personal ($29/mo):** Unlimited conversations, 1 worker, priority models,
  scheduled runs (v0.3+)
- **Team ($99/mo):** Unlimited, 5 workers, shared audit log, SSO (simple)
- **Enterprise:** Custom. Self-hosted control plane, SSO/SAML, audit
  export, on-prem deployment. Sales-assisted, post-v1.x.

---

## Categories (for applications)

- Primary: Infrastructure / Dev Tools
- Secondary: B2B / Enterprise Applications

Deliberately NOT: Deep Tech (this is advanced engineering on known
primitives, not novel science), Consumer, Gaming, Healthcare, GovTech,
Crypto.

---

## What I want from you

Produce the following deliverables in order. Write clearly. Be specific.
Where you need to make a decision, make it and note it as a decision;
where I need to decide, call it out explicitly.

### Deliverable 1: Framework specification

A single markdown document at `spec/framework.md` with these sections:

1. **Purpose** (1 page) — what UseZombie v2 is, in plain English, for a
   reader who has not heard of us.

2. **Non-goals** (half page) — what we explicitly do not do, and which
   existing tool the reader should use instead for each non-goal.

3. **Architecture** (2-4 pages) — components (zombied, zombie-worker,
   nullclaw, zombiectl, zombie, clawhub), how they interact, which
   processes run where. Include an ASCII block diagram.

4. **The credential firewall** (2 pages) — the core technical
   differentiator. Describe the placeholder pattern, the network-boundary
   proxy, how mTLS re-origination works for Kubernetes specifically,
   how bearer-token injection works for HTTP APIs. Acknowledge
   Microsandbox as prior art and describe what UseZombie adds (skill
   registry + audit + approval gates + infrastructure-specific policy).

5. **Skills specification** (2 pages) — the markdown frontmatter schema
   a skill must declare, how skills compose into zombies, the contract
   between a skill spec and the worker's generic implementation, the
   allowed-verbs policy pattern.

6. **The zombie loop** (1-2 pages) — how `zombie` runs a conversation:
   session lifecycle, skill selection, tool-call invocation, observation,
   iteration, termination. Describe the interactive REPL and the
   --once mode.

7. **Approval gates** (1 page) — how a require-approval tool call is
   paused, how approvals are surfaced to the user (v0.2 terminal/web,
   v0.2.x Slack, v0.3 mobile), how deny is handled.

8. **Audit log** (1 page) — the event schema, what gets logged, what
   doesn't, how outputs are redacted, how the log is queried.

9. **Deployment modes** (1 page) — hosted control plane + customer-run
   worker (default), fully self-hosted (control plane in customer VPC),
   local-only (single-binary mode for CI/homelab hacking).

10. **Security model** (2 pages) — threat model. What a malicious agent
    can and cannot do. What a compromised worker means. What a compromised
    control plane means. What happens if the LLM provider is compromised.
    Explicit non-defenses (we don't defend against a malicious host
    operator who can inspect their own worker's memory).

11. **Roadmap** (half page) — v2.0 (read-only skills + credential firewall
    + audit + kill switch), v2.1 (write skills with approval gates), v2.2
    (scheduled runs, Slack approvals), v2.3 (mobile push, multi-target
    correlation).

Be concrete. Include schemas, ASCII diagrams, command-line examples
where they help. Assume the reader is a technical engineer, not an
investor.

### Deliverable 2: Repo scaffolding

A tree listing at `spec/repo-layout.md` showing the target directory
structure for github.com/usezombie/usezombie. Include:

- Top-level: zombied/, zombie-worker/, nullclaw/, zombiectl/,
  clawhub/, spec/, samples/, docs/, LICENSE, README.md, CONTRIBUTING.md
- samples/ subfolders: homelab/, side-project-resurrector/,
  migration-zombie/, homebox-audit/, skills/
- Each sample and each skill gets a README.md (already partially drafted
  for homelab, kubectl-readonly, docker-readonly — this prompt has the
  full canonical versions)
- docs/ subfolders: quickstart/, guides/, reference/, blog/

Note for each folder: what goes there, who owns it, what the README
should say.

### Deliverable 3: Implementation priorities

A markdown file at `spec/implementation-plan.md` giving me a concrete
2-week solo-dev implementation plan to ship v2.0.0 with Homelab Zombie
as the demo. Break down by day. Be honest about what's cut from scope
to fit the timeline. The goal is a shippable v0.1 that:

- Boots a worker via `docker run`
- Authenticates to the control plane
- Accepts one credential (kubeconfig)
- Loads the homelab skill bundle (homelab + kubectl-readonly + docker-readonly)
- Runs a conversational `zombie` session
- Executes at least 5 read-only kubectl verbs behind the placeholder
  proxy
- Writes an audit log queryable via zombiectl
- Has a working kill switch

Anything beyond this is v2.0.1+.

---

## Constraints and reminders

- I am solo. Don't propose work that requires a team.
- I am already committed to Zig for zombied/nullclaw and JS/Bun for
  zombiectl. Don't propose a language change.
- I want open-source core (Apache-2.0) from day one.
- I don't want to build things other projects have already shipped well.
  If a primitive exists as OSS (HashiCorp Vault for secrets storage at
  rest, Firecracker/libkrun for microVMs, OpenTelemetry for audit export),
  USE IT. Don't reinvent it.
- Be direct. Push back where I'm wrong. If my architecture has a flaw —
  say it. If my pricing makes no sense — say it. Soft-pedaling is not
  useful.
- Be specific. Real schemas, real CLI commands, real file paths. Not
  "something like" or "a mechanism that."

---

## Tone guidance

Write the way a senior engineer explains a system to a new hire on day
one: direct, concrete, honest about tradeoffs. Do not write marketing
prose. Do not praise the design; describe it. Include caveats and known
weaknesses in the relevant sections, not in a separate "disclaimers"
block.

---

Start with Deliverable 1, section 1 (Purpose), and work through in order.
When you finish a section, proceed to the next without asking for
confirmation unless you hit a real ambiguity where the right answer
genuinely depends on my call — in which case, stop, state the ambiguity,
and ask.
