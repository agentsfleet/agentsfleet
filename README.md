<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg" />
  <source media="(prefers-color-scheme: light)" srcset="assets/logo-light.svg" />
  <img src="assets/logo-dark.svg" width="200" alt="usezombie" />
</picture>

**A durable, markdown-defined runtime that owns one operational outcome.** Open source · BYOK · hosted on `api.usezombie.com`.

[![CI](https://github.com/usezombie/usezombie/actions/workflows/test.yml/badge.svg?branch=main)](https://github.com/usezombie/usezombie/actions/workflows/test.yml?query=branch%3Amain)
[![Coverage](https://codecov.io/gh/usezombie/usezombie/flags/apps/graph/badge.svg)](https://codecov.io/gh/usezombie/usezombie/flags/apps)
[![Try Free](https://img.shields.io/badge/usezombie-Try_Free-brightgreen?style=for-the-badge)](https://usezombie.com)
[![Docs](https://img.shields.io/badge/usezombie-Docs-blue?style=for-the-badge)](https://docs.usezombie.com)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)

</div>

---

> **Early Access Preview** — APIs, CLI, and behavior may change before GA.

## What it does

A **Zombie** is a long-lived runtime that owns one operational outcome end-to-end. v2 ships one zombie: **`platform-ops`** — wakes on a GitHub Actions deploy-failure webhook, gathers evidence (CD logs, hosting provider, data-plane health), and posts an evidenced diagnosis to Slack. The same zombie is reachable via `zombiectl steer {id}` for manual investigation.

You don't code the zombie. You write `SKILL.md` in plain English; install it with `/usezombie-install-platform-ops` (Claude Code, Amp, Codex CLI, or OpenCode); the runtime handles the sandbox, credential injection, audit trail, budget caps, and approval gates.

## Pillars

- **OSS** — read the code that holds your credentials.
- **BYOK** — your LLM key (Anthropic, OpenAI, Together, Groq…), your inference cost.
- **Markdown-defined** — `SKILL.md` + `TRIGGER.md` as configuration; iterate prose, not control flow.
- **Sandboxed** — bwrap + landlock + cgroups; network deny-by-default.
- **Durable** — every event lands in `core.zombie_events` with actor provenance; resumable from checkpoint.

Self-host arrives in v3. v2 is hosted-only on `api.usezombie.com` via Clerk OAuth.

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
