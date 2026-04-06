<div align="center">

# UseZombie

**Submit a spec. Get a validated PR.**

[![CI](https://github.com/usezombie/usezombie/actions/workflows/test.yml/badge.svg?branch=main)](https://github.com/usezombie/usezombie/actions/workflows/test.yml?query=branch%3Amain)
[![Coverage](https://codecov.io/gh/usezombie/usezombie/flags/apps/graph/badge.svg)](https://codecov.io/gh/usezombie/usezombie/flags/apps)
[![Version](https://img.shields.io/badge/version-0.1.0-blue?style=flat-square)](https://github.com/usezombie/usezombie/releases)

[![Try Free](https://img.shields.io/badge/UseZombie-Try_Free-brightgreen?style=for-the-badge)](https://usezombie.com)
[![Docs](https://img.shields.io/badge/UseZombie-Docs-blue?style=for-the-badge)](https://docs.usezombie.com)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)

</div>

---

> 🧟 **Early Access Preview** · Pre-release — revised release coming up by April 11. APIs, CLI, and behavior may change without notice before general availability.
>
> UseZombie is in a product pivot. The focus is practical operator leverage, not tunnel-vision optimization around one narrow bottleneck that frontier models may erase soon.
>
> Write a spec. An agent implements it, runs `make lint` / `make test` / `make build` with self-repair, scores the output, and opens a PR with a scorecard. You review one PR instead of babysitting ten agent sessions.

## What it does

- **Agent sandboxes** — spin isolated sandboxes attached to your git repo, one per run
- **Spec to PR** — submit a markdown spec, an agent implements it, self-repairs until lint/test/build pass, and opens a PR
- **Scorecards** — every run produces an evidence-backed scorecard so you know exactly what passed, what was repaired, and why

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
| [docs.usezombie.com](https://docs.usezombie.com) | User-facing docs (guides, API reference) |
| [docs/operator/](docs/operator/) | Internal operator guide (deployment, config, observability, security) |
| [docs/contributing/](docs/contributing/) | Contributor guide (setup, architecture, testing, [website content](docs/contributing/website-content.md)) |
| [playbooks/](playbooks/) | Step-by-step deployment playbooks |

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

See [docs/contributing/development.md](docs/contributing/development.md) for full setup.

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
