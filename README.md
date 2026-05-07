<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg" />
  <source media="(prefers-color-scheme: light)" srcset="assets/logo-light.svg" />
  <img src="assets/logo-dark.svg" width="200" alt="usezombie" />
</picture>

**Always-on operational runtime that wakes on your events, runs against a durable replayable log, and posts evidenced answers — not chats.** Markdown-defined. Hosted on `api.usezombie.com`.

[![CI](https://github.com/usezombie/usezombie/actions/workflows/test.yml/badge.svg?branch=main)](https://github.com/usezombie/usezombie/actions/workflows/test.yml?query=branch%3Amain)
[![Coverage](https://codecov.io/gh/usezombie/usezombie/flags/apps/graph/badge.svg)](https://codecov.io/gh/usezombie/usezombie/flags/apps)
[![Try Free](https://img.shields.io/badge/usezombie-Try_Free-brightgreen?style=for-the-badge)](https://usezombie.com)
[![Docs](https://img.shields.io/badge/usezombie-Docs-blue?style=for-the-badge)](https://docs.usezombie.com)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)

</div>

---

> **Early Access Preview** — APIs, CLI, and behavior may change before GA.

Your deploy fails at 3am. Zombie wakes on the GitHub webhook, walks your CD logs + hosting + data-plane, posts the diagnosis to Slack with line-numbered evidence — every action recorded in a replayable event log. Markdown is the only thing you write.

## What it does

A **Zombie** is a long-lived runtime that owns one operational outcome end-to-end. Not request-response. Not a graph DSL. A daemon for an outcome.

v2 ships one zombie: **`platform-ops`** — wakes on a GitHub Actions deploy-failure webhook, gathers evidence (CD logs, hosting provider, data-plane health), and posts an evidenced diagnosis to Slack. The same zombie is reachable via `zombiectl steer {id}` for manual investigation.

You don't code the zombie. You write `SKILL.md` in plain English; install it with `/usezombie-install-platform-ops` (Claude Code, Amp, Codex CLI, or OpenCode); the runtime handles the sandbox, credential injection, audit trail, budget caps, and approval gates.

## Why this shape

- **Always-on, event-driven** — triggers on webhooks (CI, monitoring, custom), not on a chat box. Designed for the enterprise pattern: billions of events, narrow ownership per zombie.
- **Markdown-defined** — `SKILL.md` + `TRIGGER.md` are the configuration. Iterate the prose; the runtime owns control flow, durability, and isolation.
- **Replayable** — every event lands in `core.zombie_events` with actor provenance. Resume from checkpoint, audit any past run, replay against a new SKILL revision.
- **Sandboxed by default** — bwrap + landlock + cgroups; network deny-by-default. The OSS source IS the audit — read what holds your credentials.

BYOK is supported (Anthropic, OpenAI, Together, Groq…). Self-host arrives in v3. v2 is hosted-only on `api.usezombie.com` via Clerk OAuth, with a $5 starter credit per workspace — no card required.

## Local development

```bash
git clone https://github.com/usezombie/usezombie.git
cd usezombie
cp .env.example .env
bun install
make up        # Postgres + Redis + zombied
make test
make lint
make doctor
```

`zombiectl` defaults to `https://api.usezombie.com` so customers don't have to configure anything. To point it at the local backend (or any other environment) during development:

| Scope | How |
|---|---|
| One command | `zombiectl --api http://localhost:3000 <command>` |
| Whole shell session | `export ZOMBIE_API_URL=http://localhost:3000` in your rc (`.zshrc` / `.bashrc`) |
| Sticky per-install | `zombiectl login --api http://localhost:3000` — persists into `~/.config/zombiectl/credentials.json` |

Precedence (highest first): `--api` flag → `ZOMBIE_API_URL` env → `API_URL` env → saved `credentials.json` → default.

## Repos

| Repo | What it is |
|---|---|
| [usezombie/usezombie](https://github.com/usezombie/usezombie) | Control plane + worker + CLI (this repo). |
| [usezombie/docs](https://github.com/usezombie/docs) | User docs ([docs.usezombie.com](https://docs.usezombie.com)). |
| [usezombie/posthog-zig](https://github.com/usezombie/posthog-zig) | PostHog SDK for Zig. |

## Agent support

All agents read [AGENTS.md](AGENTS.md). Per-agent configs (`.codex/`, `.opencode/`, `.amp/`) just point back to it.

## License

MIT — Copyright (c) 2026 usezombie
