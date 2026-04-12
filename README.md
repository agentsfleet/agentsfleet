<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg" />
  <source media="(prefers-color-scheme: light)" srcset="assets/logo-light.svg" />
  <img src="assets/logo-dark.svg" width="200" alt="usezombie" />
</picture>

**Your agent is ready. You're not ready to trust it. We fix that.**

**Run your agents 24/7. Credentials hidden. Every action logged. Big moves approved.**

[![CI](https://github.com/usezombie/usezombie/actions/workflows/test.yml/badge.svg?branch=main)](https://github.com/usezombie/usezombie/actions/workflows/test.yml?query=branch%3Amain)
[![Coverage](https://codecov.io/gh/usezombie/usezombie/flags/apps/graph/badge.svg)](https://codecov.io/gh/usezombie/usezombie/flags/apps)
[![Version](https://img.shields.io/badge/version-0.5.0-blue?style=flat-square)](https://github.com/usezombie/usezombie/releases)

[![Try Free](https://img.shields.io/badge/UseZombie-Try_Free-brightgreen?style=for-the-badge)](https://usezombie.com)
[![Docs](https://img.shields.io/badge/UseZombie-Docs-blue?style=for-the-badge)](https://docs.usezombie.com)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)

</div>

---

> **Early Access Preview** · Pre-release — APIs, CLI, and behavior may change without notice before general availability.

## The problem

You have a working agent. It can reply to emails, fix bugs, process payments, review PRs. But you won't let it run unsupervised because:

- It has your API keys (what if it goes rogue?)
- You can't see what it did (what if the CEO asks?)
- There's no spend ceiling (what if one bad prompt burns $500?)
- There's no kill switch (what if it starts replying to every email in your inbox?)

So you babysit it. Or you don't run it at all.

## What Zombies are

A **Zombie** is a preconfigured agent workflow that does one job and runs forever.

```
"Install the Lead Zombie"       → handles inbound email, replies, logs leads
"Install the Slack Bug Fixer"   → monitors #bugs, opens PRs, replies in thread
"Install the PR Zombie"         → reviews every PR, posts feedback, alerts on critical
"Install the Ops Zombie"        → watches infra, alerts on incidents
"Install the Hiring Zombie"     → receives candidate profile (resume PDF, GitHub PRs,
                                   Gmail), analyzes attachments for merit, sends you
                                   a decision report on Discord
```

You don't code a Zombie. You configure it: what tools it attaches, what credentials it uses, what budget it has, what triggers it. The agent intelligence is built in.

## How it works

- **Sandboxed runtime** — bwrap + landlock isolation. Your agent runs in a locked-down process. Network deny-by-default. Only allowlisted domains reachable.
- **Credentials hidden** — agents never see API keys. The vault injects credentials at the sandbox boundary, outside the agent's process. The agent makes a tool call, UseZombie makes the real API request with the real credential.
- **Webhooks wired** — receive events from email, Slack, GitHub without ngrok or custom servers. `POST /v1/webhooks/{zombie_id}` with idempotent dedup.
- **Activity stream** — every action timestamped and queryable. `zombiectl logs` shows what happened, when, and why.
- **Spend ceiling** — per-day and per-month dollar budgets. One bad prompt never becomes an infinite burn.
- **Kill switch** — `zombiectl kill` stops any agent mid-action. Checkpoint saved, no data lost.
- **Crash recovery** — Zombie state checkpointed to Postgres after every event. Worker crashes, restarts, picks up where it left off.

## Stack

| Layer | Technology |
|-------|-----------|
| Runtime | Zig 0.15.2 (zombied server) |
| Sandbox | bwrap + landlock (Linux) |
| Agent engine | NullClaw (built-in LLM agent) |
| Database | Postgres (PlanetScale) |
| Queue | Redis Streams (Upstash) |
| Auth | Clerk |
| CLI | Node.js / Bun (zombiectl, npm package) |
| Analytics | PostHog |
| Hosting | Fly.io + bare-metal workers |

## Documentation

| Resource | Description |
|----------|-------------|
| [docs.usezombie.com](https://docs.usezombie.com) | User-facing docs (guides, API reference, operator guide) |
| [playbooks/](playbooks/) | Agent-readable deployment playbooks and gate scripts |

## Local development

```bash
git clone https://github.com/usezombie/usezombie.git
cd usezombie
cp .env.example .env
make up        # Start Postgres + Redis + zombied
make test      # Run all unit tests
make lint      # Format + lint
make doctor    # Check config, Postgres, LLM key
```

## Agent support

All agents read the canonical [AGENTS.md](AGENTS.md) operating model. Each config file below is project-specific (checked into this repo).

| Agent | Config |
|-------|--------|
| Claude Code | `AGENTS.md` |
| Codex | `.codex/instructions.md` → `AGENTS.md` |
| OpenCode | `.opencode/instructions.md` → `AGENTS.md` |
| Amp | `.amp/instructions.md` → `AGENTS.md` |

## License

MIT — Copyright (c) 2026 UseZombie
