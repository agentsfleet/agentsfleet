<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg" />
  <source media="(prefers-color-scheme: light)" srcset="assets/logo-light.svg" />
  <img src="assets/logo-dark.svg" width="200" alt="usezombie" />
</picture>

**A markdown-defined, durable, BYOK zombie that owns one operational outcome — wakes on a GitHub Actions deploy failure, gathers evidence, posts a diagnosis to your Slack.**

**Open source. Bring your own LLM key. The zombie keeps state across attempts, requests approval for risky actions, and stays on the outcome until it's resolved or explicitly blocked.**

[![CI](https://github.com/usezombie/usezombie/actions/workflows/test.yml/badge.svg?branch=main)](https://github.com/usezombie/usezombie/actions/workflows/test.yml?query=branch%3Amain)
[![Coverage](https://codecov.io/gh/usezombie/usezombie/flags/apps/graph/badge.svg)](https://codecov.io/gh/usezombie/usezombie/flags/apps)
[![Version](https://img.shields.io/badge/version-0.25.0-blue?style=flat-square)](https://github.com/usezombie/usezombie/releases)

[![Try Free](https://img.shields.io/badge/UseZombie-Try_Free-brightgreen?style=for-the-badge)](https://usezombie.com)
[![Docs](https://img.shields.io/badge/UseZombie-Docs-blue?style=for-the-badge)](https://docs.usezombie.com)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)

</div>

---

> **Early Access Preview** · Pre-release — APIs, CLI, and behavior may change without notice before general availability.

## The problem

A deploy fails at 2am. Your CD pipeline goes red. Now you're bouncing between five tools while holding the timeline in your head: GitHub Actions for the run logs, Grafana for the metrics, your observability dashboard for the traces, Slack for the alerts, your terminal for `kubectl` and `flyctl`. You correlate, you guess, you restart something with no clear root cause, and the next morning you can't reconstruct what you did.

The work is fragmented. State is lost between attempts. There is no durable memory across incidents. And there is no automatic mechanism to keep your team or your customers informed while you triage.

## What v2 does

A **Zombie** is a long-lived runtime that owns one operational outcome end-to-end.

The v2 wedge ships one zombie: **`platform-ops`** — wakes on a GitHub Actions deploy-failure webhook, gathers evidence from your CD logs, your hosting provider, and your data-plane, then posts an evidenced diagnosis to Slack. Same zombie is reachable manually for a "morning health check" or any operator-driven investigation via `zombiectl steer {id}`.

```
GitHub Actions: workflow_run.conclusion=failure
   ↓
Zombie wakes, gathers evidence (Fly logs, Upstash health, GH run logs)
   ↓
NullClaw reasons over the message inside a Landlock+cgroups+bwrap sandbox
   ↓
Posts evidenced diagnosis to your Slack channel
   ↓
Every event lands in core.zombie_events with actor provenance
```

You don't code the zombie. You write a SKILL.md in plain English, install it with `/usezombie-install-platform-ops` (Claude Code, Amp, Codex CLI, or OpenCode), and the runtime handles the sandbox, credential injection, audit trail, budget caps, and approval gates.

## How it works

- **Sandboxed runtime** — bwrap + landlock + cgroups. The zombie's agent runs in a locked-down process. Network deny-by-default; only the hosts in `network.allow` are reachable.
- **Credentials hidden** — the agent never sees API keys. The vault stores structured `{host, api_token}` records, KMS-enveloped. The tool bridge substitutes `${secrets.NAME.FIELD}` placeholders with real bytes AFTER sandbox entry, on the request line. Tokens never enter the agent's LLM context.
- **Webhooks wired** — `POST /v1/.../webhooks/github` with HMAC verification lands as a synthetic event with `actor=webhook:github`. Same reasoning loop as a manual steer; only the actor differs.
- **Durable history** — every event (webhook / cron / steer) lands in `core.zombie_events` with actor provenance. Resumable from checkpoint after worker restart.
- **BYOK** — bring your own LLM provider key (Anthropic, OpenAI, Together, Groq). Resolved via the same vault as your other credentials. No vendor lock-in on inference cost.
- **Markdown-defined** — operational behavior lives in `SKILL.md` + `TRIGGER.md` (or merged frontmatter under `x-usezombie:`). Iterate by editing prose, not by redeploying code.
- **Context lifecycle** — long incidents don't crash the model. Three layers compose: rolling tool-result window, periodic `memory_store` checkpoints, and stage chunking with continuation events when context fills past threshold. See [`docs/ARCHITECHTURE.md`](docs/ARCHITECHTURE.md) §11.
- **Budget caps + kill switch** — daily and monthly dollar caps; `zombiectl kill` halts any zombie mid-action with checkpoint saved.

## Differentiation (v2)

Three structural pillars:

- **OSS** — read the code that holds your credentials.
- **BYOK** — your LLM key, your inference cost.
- **Markdown-defined** — `SKILL.md` + `TRIGGER.md` as configuration; iterate prose, not typed control flow.

Self-host arrives in v3. v2 ships hosted-only on `api.usezombie.com` via Clerk OAuth.

## Stack

| Layer | Technology |
|-------|-----------|
| Runtime | Zig 0.15.2 (zombied server) |
| Sandbox | bwrap + landlock + cgroups (Linux) |
| Agent engine | NullClaw (built-in LLM agent loop) |
| Database | Postgres (PlanetScale) |
| Queue | Redis Streams (Upstash) |
| Auth | Clerk OAuth |
| CLI | Node.js / Bun (zombiectl, npm package) |
| Install UX | `/usezombie-install-platform-ops` SKILL.md (Claude Code / Amp / Codex CLI / OpenCode) |
| Analytics | PostHog |
| Hosting | Fly.io |

## Documentation

| Resource | Description |
|----------|-------------|
| [docs.usezombie.com](https://docs.usezombie.com) | User-facing docs (guides, API reference, operator guide) — sources at [usezombie/docs](https://github.com/usezombie/docs) |
| [playbooks/](playbooks/) | Agent-readable deployment playbooks and gate scripts |
| [docs/v2/](docs/v2/) | Milestone specs (pending / active / done). The unit of planning in this repo — every non-trivial change flows through one. |

## Related repositories

| Repo | Description |
|------|-------------|
| [usezombie/docs](https://github.com/usezombie/docs) | The user docs site. Publishes to `docs.usezombie.com`. Every user-visible release lands a `<Update>` block in its `changelog.mdx`. |
| [usezombie/posthog-zig](https://github.com/usezombie/posthog-zig) | PostHog SDK for Zig. Vendored here to power server-side analytics — the signup funnel, zombie triggers, billing events. |
| [usezombie/.github](https://github.com/usezombie/.github) | Organization profile (landing README shown on the org page). |

## Local development

```bash
git clone https://github.com/usezombie/usezombie.git
cd usezombie
cp .env.example .env
bun install    # Hydrate workspace packages (ui/, zombiectl/) — required before `make lint`
make up        # Start Postgres + Redis + zombied
make test      # Run all unit tests
make lint      # Format + lint (website ESLint needs `bun install` first)
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
