<div align="center"><img src="branding/agentsfleet-mark-glow.png" width="180" alt="agentsfleet" />

# A fleet of prebuilt AI teammates for recurring engineering work.

[![CI](https://img.shields.io/github/actions/workflow/status/agentsfleet/agentsfleet/test.yml?branch=main&label=CI&logo=github&logoColor=white)](https://github.com/agentsfleet/agentsfleet/actions/workflows/test.yml?query=branch%3Amain)
[![zig-agentsfleetd coverage](https://img.shields.io/codecov/c/github/agentsfleet/agentsfleet?flag=zig-agentsfleetd&label=zig-agentsfleetd&logo=codecov&logoColor=white)](https://codecov.io/gh/agentsfleet/agentsfleet?flags[0]=zig-agentsfleetd)
[![zig-runner coverage](https://img.shields.io/codecov/c/github/agentsfleet/agentsfleet?flag=zig-runner&label=zig-runner&logo=codecov&logoColor=white)](https://codecov.io/gh/agentsfleet/agentsfleet?flags[0]=zig-runner)
[![zig-lib coverage](https://img.shields.io/codecov/c/github/agentsfleet/agentsfleet?flag=zig-lib&label=zig-lib&logo=codecov&logoColor=white)](https://codecov.io/gh/agentsfleet/agentsfleet?flags[0]=zig-lib)
[![app coverage](https://img.shields.io/codecov/c/github/agentsfleet/agentsfleet?flag=app&label=app&logo=codecov&logoColor=white)](https://codecov.io/gh/agentsfleet/agentsfleet?flags[0]=app)
[![website coverage](https://img.shields.io/codecov/c/github/agentsfleet/agentsfleet?flag=website&label=website&logo=codecov&logoColor=white)](https://codecov.io/gh/agentsfleet/agentsfleet?flags[0]=website)
[![cli coverage](https://img.shields.io/codecov/c/github/agentsfleet/agentsfleet?flag=cli&label=cli&logo=codecov&logoColor=white)](https://codecov.io/gh/agentsfleet/agentsfleet?flags[0]=cli)
[![Zig](https://img.shields.io/badge/Zig-0.16.0-F7A41D?logo=zig&logoColor=white)](https://ziglang.org/download/)
[![Docs](https://img.shields.io/badge/Docs-agentsfleet.net-0A7CFF?logo=gitbook&logoColor=white)](https://docs.agentsfleet.net)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

</div>

**[agentsfleet](https://agentsfleet.net)** is a fleet of prebuilt AI teammates for recurring engineering work. Each one wakes on an event — a pull request, an incident, a deploy — reads your code, telemetry, internal docs, and live control-plane state, finds the root cause, and opens a scenario-backed fix. A human approves, then it ships the fix or drafts the customer reply. Every step is a replayable log.

- **Human approval, by design** — the agent investigates and proposes; a person approves before anything ships
- **Replayable event logs** — audit every action and decision
- **Bring your own provider keys** — no vendor lock-in on inference
- **Runs locally or against production** — same agent, same evidence

Agents are defined in Markdown playbooks with tools, triggers, and investigation steps. Open-source runtime, hosted control plane — the teammate, not a wrapper around someone else's.

---

## Quick start

```bash
npm install -g @agentsfleet/cli
agentsfleet login
```

Define an agent in Markdown, connect a webhook, and get an evidenced diagnosis and a proposed fix on your next pull request or incident. Full walkthrough at **[docs.agentsfleet.net/quickstart](https://docs.agentsfleet.net/quickstart)** — free to try, no card, under five minutes.

---

## What's in this repo

| Directory | What |
|---|---|
| `src/` | Zig backend — `agentsfleetd` control plane (HTTP, leases) + `agentsfleet-runner` execution daemon |
| `ui/packages/app/` | Dashboard — Next.js, Clerk auth |
| `ui/packages/website/` | Marketing site — [agentsfleet.net](https://agentsfleet.net) |
| `ui/packages/design-system/` | Shared UI components |
| `cli/` | Command-line interface (CLI) — install, manage agents, tail runs |
| `public/openapi/` | OpenAPI spec |
| `schema/` | Postgres migrations |
| `playbooks/` | First setup, deployment, verification, and operations |

---

## Local development

**Prerequisites:** [Zig 0.16.0](https://ziglang.org/download/) · [Docker](https://www.docker.com) (Postgres + Redis) · [Bun ≥1.3](https://bun.sh) · [Clerk](https://clerk.com) dev project · [1Password CLI](https://1password.com/downloads/command-line/) for secrets

```bash
git clone https://github.com/agentsfleet/agentsfleet.git
cd agentsfleet

# Populate .env before running make up. See playbooks/founding/01_bootstrap/001_playbook.md for the full bootstrap.
make up           # Postgres + Redis + agentsfleetd (auto-migrates DB)

cd ui/packages/app
echo "NEXT_PUBLIC_API_URL=http://localhost:3000" > .env.local
bun install && bun run dev
```

**Verify:**

```bash
make lint-all
make test-unit-all
make test-integration   # needs make up running
```

---

## CLI

`agentsfleet` defaults to **production**. Point it at your local stack with the `--api` flag, or persist it via the environment:

```bash
agentsfleet --api http://localhost:3000 <command>
export AGENTSFLEET_API_URL=http://localhost:3000   # or set it once
```

---

## Contributing

```bash
git config core.hooksPath .githooks
```

Project facts live in [`AGENTS.md`](AGENTS.md). The operating model and the
deterministic gate scripts are committed alongside them —
[`AGENTS.orly.md`](AGENTS.orly.md), `dispatch/`, and `audits/` — materialised
by [orly](https://github.com/agentsfleet/orly) and recorded in
`.oracle/orly.json`. A clone carries its own rules; nothing resolves out of a
developer's home directory.

```bash
bunx @agentsfleet/orly update   # re-materialise at the installed version
orly doctor                     # report drift between the lock and the tree
```

`make harness-verify` runs the gates from `audits/` in this repository.

Read about [architecture](docs/architecture/), start with the
[operator playbooks](playbooks/README.md), or jump into
[local development](#local-development).

---

MIT — Copyright (c) 2026 agentsfleet.
