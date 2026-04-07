<div align="center">

# UseZombie

**Submit a spec. Get a validated PR.**

[![CI](https://github.com/usezombie/usezombie/actions/workflows/test.yml/badge.svg?branch=main)](https://github.com/usezombie/usezombie/actions/workflows/test.yml?query=branch%3Amain)
[![Coverage](https://codecov.io/gh/usezombie/usezombie/flags/apps/graph/badge.svg)](https://codecov.io/gh/usezombie/usezombie/flags/apps)
[![Version](https://img.shields.io/badge/version-0.4.0-blue?style=flat-square)](https://github.com/usezombie/usezombie/releases)

[![Try Free](https://img.shields.io/badge/UseZombie-Try_Free-brightgreen?style=for-the-badge)](https://usezombie.com)
[![Docs](https://img.shields.io/badge/UseZombie-Docs-blue?style=for-the-badge)](https://docs.usezombie.com)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)

</div>

---

> **Early Access Preview** · Pre-release — revised release coming up by April 11. APIs, CLI, and behavior may change without notice before general availability.

## Why we pivoted

We started by obsessing over one thing: making AI-generated code *correct*. Self-repair loops, quality scoring, scorecard evidence. Good problems — but narrow ones. Frontier models get better every quarter, and the gap we were optimizing for keeps shrinking on its own.

The harder problem nobody is solving: **operators are still babysitting agents like pets.**

```
          ____________________________
         < code(human) === cattle now >
          ----------------------------
                 \   ^__^
                  \  (oo)\_______
                     (__)\       )\/\
                         ||----w |
                         ||     ||
```

So we pivoted. UseZombie is now a runtime for always-on agents — you bring your agent, we handle the credentials (hidden from the sandbox, injected at the firewall), webhooks (wired automatically), audit logs (every action timestamped), and a kill switch. Your agent runs 24/7 without ever seeing a password.

## What zombies do now

- **Always-on agents** — your agent runs continuously in a sandboxed process, restarts on crash
- **Credentials hidden** — agents never see tokens; the firewall injects them per-request, outside the sandbox boundary
- **Webhooks wired** — receive events from email, Slack, GitHub, etc. without ngrok or custom servers
- **Audit everything** — every request, webhook, and credential use is timestamped and replayable
- **Kill switch** — stop any agent mid-action from the CLI or web UI

## Stack

- **Auth** — [Clerk](https://clerk.com)
- **Hosting** — [Fly.io](https://fly.io) + bare-metal workers
- **Database** — [PlanetScale](https://planetscale.com) (Postgres)
- **Queue** — [Upstash](https://upstash.com) Redis Streams
- **Analytics** — [PostHog](https://posthog.com)
- **Email** — [Resend](https://resend.com)

## Documentation

| Resource | Description |
|----------|-------------|
| [docs.usezombie.com](https://docs.usezombie.com) | User-facing docs (guides, API reference, operator guide, contributing) |
| [playbooks/](playbooks/) | Agent-readable deployment playbooks and gate scripts |

## Playbooks

Playbooks are written to be understandable and executable by both humans and agents — each directory contains a markdown doc (`001_playbook.md`) and numbered shell scripts (`00_gate.sh`, `01_*.sh`, `02_*.sh`) that run in order.

See [playbooks/README.md](playbooks/README.md) for the full index and execution order.

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

See [docs.usezombie.com/contributing/development](https://docs.usezombie.com/contributing/development) for full setup.

## Agent support

This repo is configured for Claude Code, Codex, OpenCode, and Amp. All agents read the canonical [AGENTS.md](AGENTS.md) operating model.

| Agent | Config |
|-------|--------|
| Claude Code | `CLAUDE.md` → `AGENTS.md` |
| Codex | `.codex/instructions.md` |
| OpenCode | `.opencode/instructions.md` → `AGENTS.md` |
| Amp | `.amp/instructions.md` → `AGENTS.md` |

## License

MIT — Copyright (c) 2026 UseZombie
