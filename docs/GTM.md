# GTM: UseZombie — Agent Delivery Control Plane

Date: Mar 2, 2026
Status: v1 GTM baseline — Factory.ai-inspired positioning

## Category and Positioning

1. Category: **Agent Delivery Control Plane.**
2. Entry message: "Deterministic execution for autonomous engineering teams."
3. Technical promise: spec-driven pipelines, policy controls, replayable artifacts, and validated PRs — from spec to production.
4. Not "CI/CD for agents" (too narrow). Not "better coding agent" (too crowded). The control plane that governs how agent work ships.

## The Problem We Solve

Software teams are adopting AI coding agents — Claude Code, Codex, Amp, Cursor — but using them one task at a time, with a human acting as router, reviewer, and PR shepherd:

1. Human picks next spec from a backlog
2. Human pastes spec into a coding agent
3. Agent generates code
4. Human reviews, spots issues, feeds corrections back
5. Human creates branch, commits, opens PR
6. Human moves to next spec
7. Repeat until exhaustion

This is **human-as-glue** between specs and pull requests. It doesn't scale. A solo builder with 5 repos and 40 specs spends more time routing than thinking. A team of 10 engineers with 200 specs across 30 repos is drowning.

No tool today connects a **spec queue** to a **coordinated agent team** to a **shipped PR** with retry loops, validation, feedback capture, and metering.

## What Makes UseZombie Different

> Technical details: `docs/ARCHITECTURE.md`. This section is positioning language for buyers.

1. **Spec-driven, not prompt-driven.** Specs live in the repo as ordered markdown files. Any tool (Codex, Amp, Claude Code, a human) can author them. UseZombie processes them in sequence.
2. **Dynamic agent pipeline, not a single agent.** A composable pipeline of agents with distinct roles — planner, builder, reviewer — coordinated by a deterministic state machine with bounded retries. Built-in personas ship with every workspace; operators can compose their own.
3. **One binary, zero dependencies.** One Zig binary (~2-3MB) with built-in agent runtime, HTTP API, and OS-level sandbox. Deploy by copying a file. No containers, no runtime dependencies.
4. **Feedback as artifacts.** Every validation failure produces a structured defect report. Auditable paper trail. Agents learn from their own mistakes across retries.
5. **"Isolate the agent" security model.** Zero secrets in the sandbox. Control plane acts as credential proxy. Agent receives only a session token and callback URL.
6. **VM-level sandboxing.** Firecracker microVM isolation provides stronger execution boundaries for untrusted code while preserving deterministic worker behavior.

## Target Use Cases

### Solo builder with multiple repos
Connect repos as workspaces. Queue specs. Come back to PRs with implementation summaries, validation reports, and defect histories.

### Platform team with shared repos
Each engineer's specs queue independently per workspace. UseZombie processes them in order, ships PRs, notifies the team. Engineers review PRs instead of babysitting agents.

### Agent-to-agent pipeline
An AI PM agent writes specs based on user feedback, drops `PENDING_*.md` files into the repo. UseZombie implements them. The PM agent reviews the PR summary — approves or drops revision spec. Zero humans in the loop.

## ICP (by version)

### v1: Solo builders and small teams
1. Solo builders running 3+ repos with AI coding agents.
2. Small teams (2-10 engineers) already using Claude Code, Codex, or Amp but babysitting each run.
3. Repos with recurring spec backlogs and frequent PR demand.
4. CLI-first delivery: `npx zombiectl login` → workspace add → specs sync → run → PR.

### v2: Teams with security requirements
1. Teams needing VM-level execution isolation (Firecracker).
2. Organizations requiring native git operations (libgit2, no subprocess).
3. Multi-worker horizontal scaling with bounded concurrency.

### v3: Platform teams and enterprise
1. Platform/DevEx teams in 20-200 engineer orgs.
2. Teams needing audit trails, policy controls, and reliability SLOs for agent-generated PRs.
3. Organizations with compliance requirements around AI-generated code.
4. Mission Control UI (`app.usezombie.com`) for visual management.

## Buyer and User Map

1. Economic buyer: Head of Platform / VP Engineering.
2. Technical champion: Staff/Principal Platform Engineer.
3. Security approver: Security/compliance stakeholder.
4. Daily users: tech leads and senior ICs.

## Core Pain Metric

Primary:
1. Human babysitting time per successful PR.

Secondary:
1. Blocked-run rate.
2. Retries per successful PR.
3. Median spec-to-PR duration.

## Packaging Model

### Free (individual)
1. One workspace. Low run/concurrency caps.
2. Safe/read-heavy command surface.
3. Short retention.

### Pro (individual)
1. Higher run caps + queue priority.
2. Longer retention. Advanced replay and cost views.
3. Sensitive commands with confirmations.

### Team
1. Shared workspaces + team-level access control.
2. Policy approvals and audit exports.
3. Team observability and alerting.

### Enterprise
1. Compliance controls, data governance.
2. Firecracker microVM execution isolation.
3. Contractual SLA/support.

## Revenue Model: BYOK + Compute Billing

### Bring Your Own Key (BYOK)
1. Users provide their own LLM API keys (Anthropic, OpenAI, Google, etc.).
2. UseZombie never touches LLM billing. Token costs flow directly to the user's provider account.
3. NullClaw supports 22+ LLM providers natively — no UseZombie-side adapter needed.

### Compute billing (UseZombie's revenue line)
1. Charge per **agent-second**: wall-clock time the worker runs Echo + Scout + Warden.
2. Example: a run takes 90 seconds across 3 NullClaw invocations → billed for 90 agent-seconds.
3. Pricing tiers control concurrency and queue priority, not per-second rates.

### Activation fee
1. $5 one-time activation per workspace.
2. Validates: LLM key works, repo access confirmed, first spec parsed.

### Unit economics target
1. Compute cost per run: $0.01-0.05 (NullClaw on OVH bare-metal — minimal memory footprint).
2. LLM cost per run: $1-5 (paid by user via BYOK).
3. UseZombie margin is on compute markup only — no LLM cost exposure.
4. Break-even at ~500 runs/month on a single OVH node.

## Website Direction (Factory.ai-inspired)

### Aesthetic
- Industrial monochrome. Dark backgrounds, technical credibility.
- Palette: `--bg-0: #020202`, `--accent: #f97316` (orange), Inter + Geist Mono.
- Technical, not marketing. Declarative, not persuasive.
- Code blocks and terminal output as visual elements.
- No "powered by AI" or "unlock the future" language.

### Routes
| Route | Purpose | Tone |
|-------|---------|------|
| `/` | Human buyer landing | "Deterministic execution for autonomous engineering teams" |
| `/agents` | Machine-readable onboarding | Terminal-style, copy-paste commands, API contract links |
| `/pricing` | Plan comparison | Clean table, BYOK explained, agent-second billing |
| `/docs` | Technical documentation | Mintlify-style |

### Machine-readable surfaces
- `openapi.json` — OpenAPI 3.1 spec
- `agent-manifest.json` — JSON-LD agent/skill discovery
- `llms.txt` — LLM-friendly site summary
- `skill.md` — Natural language onboarding for LLM agents
- `/agents` route with JSON-LD + code examples

## v1 Launch Domains

| Domain | Purpose |
|---|---|
| `usezombie.com` | Human buyer landing page + pricing |
| `usezombie.sh` | Agent discovery, machine-readable onboarding |
| `docs.usezombie.com` | Mintlify-hosted technical documentation |
| `api.usezombie.com` | zombied API endpoint |
| `app.usezombie.com` | v3 — Mission Control UI (not v1) |

## Agent Readiness (v2 differentiator)

Factory.ai-style repository assessment:
1. Evaluate repos across technical pillars (style/validation, build system, testing, docs, dev environment, code quality, observability, security).
2. Five maturity levels. Level 3 = "production-ready for agents."
3. Automated remediation: fix failing criteria via PR.
4. Compounds agent effectiveness — a differentiator vs raw agent tools.

## Messaging Rules

1. Human route (`/`): explain value in delivery reliability, governance, and throughput.
2. Agent route (`/agents`): machine-readable onboarding and API discoverability.
3. Never mix channel ergonomics with control-plane truth; keep one backend contract.
4. Show the product. Don't describe it. Terminal recordings and code blocks as primary visuals.
