<div align="center"><img src="branding/usezombie-mark-glow.png" width="180" alt="usezombie" />

# Your deploy failed. The agent already knows why.

[![CI](https://github.com/usezombie/usezombie/actions/workflows/test.yml/badge.svg?branch=main)](https://github.com/usezombie/usezombie/actions/workflows/test.yml?query=branch%3Amain)
[![Docs](https://img.shields.io/badge/Docs-blue)](https://docs.usezombie.com)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

</div>

**[usezombie](https://usezombie.com)** automates incident investigation. When a deploy fails, a zombie wakes — gathers evidence from your logs, metrics, health endpoints, and recent commits — then posts a diagnosis to Slack with a replayable event log.

- **Replayable event logs** — audit every action and decision
- **Bring your own provider keys** — no vendor lock-in on inference
- **Runs locally or against production** — same zombie, same evidence

Agents are defined in Markdown playbooks with tools, triggers, and investigation steps. Open-source runtime, hosted control plane.

---

## Quick start

```bash
bun install -g zombiectl
zombiectl login
```

Define a zombie in Markdown, connect a webhook, and get a Slack diagnosis on your next deploy failure. Full walkthrough at **[docs.usezombie.com/quickstart](https://docs.usezombie.com/quickstart)** — free to try, no card, under five minutes.

---

## What's in this repo

| Directory | What |
|---|---|
| `src/` | Zig backend — `zombied` control plane (HTTP, leases) + `zombie-runner` execution daemon |
| `ui/packages/app/` | Dashboard — Next.js, Clerk auth |
| `ui/packages/website/` | Marketing site — [usezombie.com](https://usezombie.com) |
| `ui/packages/design-system/` | Shared UI components |
| `zombiectl/` | CLI — install, manage zombies, tail runs |
| `public/openapi/` | OpenAPI spec |
| `schema/` | Postgres migrations |

---

## Local development

**Prerequisites:** [Zig 0.15.2](https://ziglang.org/download/) · [Docker](https://www.docker.com) (Postgres + Redis) · [Bun ≥1.3](https://bun.sh) · [Clerk](https://clerk.com) dev project · [1Password CLI](https://1password.com/downloads/command-line/) for secrets

```bash
git clone https://github.com/usezombie/usezombie.git
cd usezombie

# Populate .env before running make up. See playbooks/001_bootstrap/001_playbook.md for the full bootstrap.
make up           # Postgres + Redis + zombied (auto-migrates DB)

cd ui/packages/app
echo "NEXT_PUBLIC_API_URL=http://localhost:3000" > .env.local
bun install && bun run dev
```

**Verify:** `make lint-all` · `make test-unit-all` · `make test-integration` (needs `make up` running).

`zombiectl` defaults to production; point it at local with `--api http://localhost:3000` or `export ZOMBIE_API_URL=http://localhost:3000`.

---

## Contributing

Enable git hooks: `git config core.hooksPath .githooks`

Bootstrap steps and coding conventions live in [`playbooks/`](playbooks/) and [`AGENTS.md`](AGENTS.md).

---

## Repos

| Repo | What |
|---|---|
| [usezombie/usezombie](https://github.com/usezombie/usezombie) | Control plane + runner + CLI (this repo) |
| [usezombie/docs](https://github.com/usezombie/docs) | User docs ([docs.usezombie.com](https://docs.usezombie.com)) |
| [usezombie/skills](https://github.com/usezombie/skills) | Agent skill libraries |
| [usezombie/posthog-zig](https://github.com/usezombie/posthog-zig) | PostHog SDK for Zig |

MIT — Copyright (c) 2026 usezombie.
