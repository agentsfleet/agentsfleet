<div align="center"><img src="branding/agentsfleet-mark-glow.png" width="180" alt="agentsfleet" />

# Your deploy failed. The agent already knows why.

[![CI](https://github.com/agentsfleet/agentsfleet/actions/workflows/test.yml/badge.svg?branch=main)](https://github.com/agentsfleet/agentsfleet/actions/workflows/test.yml?query=branch%3Amain)
[![Docs](https://img.shields.io/badge/Docs-blue)](https://docs.agentsfleet.net)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

</div>

**[agentsfleet](https://agentsfleet.net)** automates incident investigation. When a deploy fails, an agent wakes — gathers evidence from your logs, metrics, health endpoints, and recent commits — then posts a diagnosis to Slack with a replayable event log.

- **Replayable event logs** — audit every action and decision
- **Bring your own provider keys** — no vendor lock-in on inference
- **Runs locally or against production** — same agent, same evidence

Agents are defined in Markdown playbooks with tools, triggers, and investigation steps. Open-source runtime, hosted control plane.

---

## Quick start

```bash
bun install -g agentsfleet
agentsfleet login
```

Define an agent in Markdown, connect a webhook, and get a Slack diagnosis on your next deploy failure. Full walkthrough at **[docs.agentsfleet.net/quickstart](https://docs.agentsfleet.net/quickstart)** — free to try, no card, under five minutes.

---

## What's in this repo

| Directory | What |
|---|---|
| `src/` | Zig backend — `agentsfleetd` control plane (HTTP, leases) + `agentsfleet-runner` execution daemon |
| `ui/packages/app/` | Dashboard — Next.js, Clerk auth |
| `ui/packages/website/` | Marketing site — [agentsfleet.net](https://agentsfleet.net) |
| `ui/packages/design-system/` | Shared UI components |
| `agentsfleet/` | CLI — install, manage agents, tail runs |
| `public/openapi/` | OpenAPI spec |
| `schema/` | Postgres migrations |

---

## Local development

**Prerequisites:** [Zig 0.15.2](https://ziglang.org/download/) · [Docker](https://www.docker.com) (Postgres + Redis) · [Bun ≥1.3](https://bun.sh) · [Clerk](https://clerk.com) dev project · [1Password CLI](https://1password.com/downloads/command-line/) for secrets

```bash
git clone https://github.com/agentsfleet/agentsfleet.git
cd agentsfleet

# Populate .env before running make up. See playbooks/founding/01_bootstrap/001_playbook.md for the full bootstrap.
make up           # Postgres + Redis + agentsfleetd (auto-migrates DB)

cd ui/packages/app
echo "NEXT_PUBLIC_API_URL=http://localhost:3000" > .env.local
bun install && bun run dev
```

**Verify:** `make lint-all` · `make test-unit-all` · `make test-integration` (needs `make up` running).

`agentsfleet` defaults to production; point it at local with `--api http://localhost:3000` or `export AGENTSFLEET_API_URL=http://localhost:3000`.

---

## Contributing

Enable git hooks: `git config core.hooksPath .githooks`

Bootstrap steps and coding conventions live in [`playbooks/`](playbooks/) and [`AGENTS.md`](AGENTS.md).

---

## Repos

| Repo | What |
|---|---|
| [agentsfleet/agentsfleet](https://github.com/agentsfleet/agentsfleet) | Control plane + runner + CLI (this repo) |
| [agentsfleet/docs](https://github.com/agentsfleet/docs) | User docs ([docs.agentsfleet.net](https://docs.agentsfleet.net)) |
| [agentsfleet/skills](https://github.com/agentsfleet/skills) | Agent skill libraries |
| [agentsfleet/posthog-zig](https://github.com/agentsfleet/posthog-zig) | PostHog SDK for Zig |

MIT — Copyright (c) 2026 agentsfleet.
