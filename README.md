<div align="center">

# UseZombie

**Agent Delivery Control Plane — one Zig binary that takes a spec and ships a validated PR.**

[![CI](https://github.com/usezombie/usezombie/actions/workflows/test.yml/badge.svg?branch=main)](https://github.com/usezombie/usezombie/actions/workflows/test.yml?query=branch%3Amain)
[![Coverage](https://codecov.io/gh/usezombie/usezombie/flags/apps/graph/badge.svg)](https://codecov.io/gh/usezombie/usezombie/flags/apps)
[![Version](https://img.shields.io/badge/version-0.1.0-blue?style=flat-square)](https://github.com/usezombie/usezombie/releases)

[![Try Free](https://img.shields.io/badge/UseZombie-Try_Free-brightgreen?style=for-the-badge)](https://usezombie.com)
[![Docs](https://img.shields.io/badge/UseZombie-Docs-blue?style=for-the-badge)](https://docs.usezombie.com)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)

</div>

---

> **In Development** — APIs, CLI, and behavior may change without notice.

## What it does

Drop a `PENDING_*.md` spec into your repo → UseZombie picks it up, runs a dynamic agent pipeline, and opens a validated PR. No babysitting.

Inspired by [L8 — Build your own orchestrator](https://x.com/garrytan/status/2033729112117018821?s=20) — currently being tested on personal projects. Agents are dynamic; built-ins ship with planner, builder, and reviewer personas but the pipeline is fully composable.

## Stack

Opinionated by default. UseZombie is built and tested against:

- **Auth** — [Clerk](https://clerk.com)
- **Hosting** — [Fly.io](https://fly.io) · Baremetal
- **Database** — [PlanetScale](https://planetscale.com) (Postgres)
- **Queue** — [Upstash](https://upstash.com) Redis Streams
- **Analytics** — [PostHog](https://posthog.com)
- **Email** — [Resend](https://resend.com)

You can swap any layer — start with the playbooks below to understand the seams.

## Playbooks

Step-by-step guides for bootstrapping, credentials, and deployment:

- [M1 — Bootstrap](docs/M1_001_PLAYBOOK_BOOTSTRAP.md)
- [M2 — Preflight](docs/M2_001_PLAYBOOK_PREFLIGHT.md)
- [M2 — Priming Infra](docs/M2_002_PLAYBOOK_PRIMING_INFRA.md)
- [M3 — Deploy Dev](docs/M3_001_PLAYBOOK_DEPLOY_DEV.md)
- [M3 — Deploy Prod](docs/M3_002_PLAYBOOK_DEPLOY_PROD.md)

## Local Development

```bash
git clone https://github.com/usezombie/usezombie.git
cd usezombie
cp .env.example .env
```

```bash
make up                # Start Postgres + Redis + zombied
make down              # Stop all services
make test              # Run unit tests + backend e2e
make test-integration  # Run all integration tests (Zig + DB + Redis via docker compose)
make lint              # Format + lint
make doctor            # Check config, Postgres, LLM key
```

## License

MIT — Copyright (c) 2026 UseZombie
