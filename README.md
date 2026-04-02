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

> **Pre-release: April 5, 2026** — UseZombie is launching April 5. APIs, CLI, and behavior may change without notice before general availability.

## What it does

UseZombie turns markdown specs into validated pull requests with self-repairing agents, run quality scoring, and evidence-backed scorecards. No babysitting.

Write a spec. An agent implements it, runs `make lint` / `make test` / `make build` with self-repair, scores the output, and opens a PR with a scorecard. You review one PR instead of babysitting ten agent sessions.

## Quick start

```bash
npm install -g zombiectl
zombiectl login
zombiectl workspace add https://github.com/your-org/your-repo
zombiectl run --spec docs/spec/my-feature.md
```

Full quickstart and CLI reference at [docs.usezombie.com](https://docs.usezombie.com).

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

- [M1 — Bootstrap](playbooks/M1_001_BOOTSTRAP.md)
- [M2 — Preflight](playbooks/M2_001_PREFLIGHT.md)
- [M2 — Priming Infra](playbooks/M2_002_PRIMING_INFRA.md)
- [M3 — Deploy Dev](playbooks/M3_001_DEPLOY_DEV.md)
- [M3 — Deploy Prod](playbooks/M3_002_DEPLOY_PROD.md)
- [M4 — Worker Bootstrap Dev](playbooks/M4_001_WORKER_BOOTSTRAP_DEV.md)
- [M7 — Credential Rotation Dev](playbooks/M7_002_CREDENTIAL_ROTATION_DEV.md)

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
