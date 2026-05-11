<div align="center">

<img src="branding/usezombie-mark-glow.png" width="180" alt="usezombie" />

# Your deploy failed. The agent already knows why.

[![Try Free — $5 Credit](https://img.shields.io/badge/usezombie-Try_Free_·_%245_Credit-5EEAD4?style=for-the-badge)](https://usezombie.com)
[![Docs](https://img.shields.io/badge/Docs-blue?style=for-the-badge)](https://docs.usezombie.com)
[![CI](https://github.com/usezombie/usezombie/actions/workflows/test.yml/badge.svg?branch=main)](https://github.com/usezombie/usezombie/actions/workflows/test.yml?query=branch%3Amain)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)

</div>

---

> **Early Access Preview.** APIs and CLI may change before GA.

A **Zombie** wakes on your events (webhook · cron · steer), gathers evidence against your infra, posts an evidenced diagnosis to Slack, and records every action in a replayable event log. Markdown-defined. Self-managed provider keys. Hosted on `api.usezombie.com`.

**Trying it as a user?** Skip the rest of this README and go straight to **[docs.usezombie.com/quickstart](https://docs.usezombie.com/quickstart)** — `$5` starter credit on signup, no card required, full install + first run in under five minutes.

---

# Local development

This repo is the control plane (Zig backend), the worker, the marketing site, and the dashboard app. Setting it up locally needs a Zig toolchain, Docker for Postgres + Redis, and a Clerk dev project for auth.

## Prereqs

| Tool | Version | Why |
|---|---|---|
| `zig` | `0.15.2` | Backend + CLI build target. `mise install` reads `mise.toml` and pulls the right version. |
| Docker | latest | Postgres + Redis brought up by `make up`. Colima or Docker Desktop both work. |
| `bun` | `≥1.3` | Workspace install + frontend dev server. |
| Clerk dev instance | one project, dev keys | Bootstrapped per [`playbooks/001_bootstrap/001_playbook.md`](playbooks/001_bootstrap/001_playbook.md) §1.2. Hand the **Publishable key + Secret key** to the agent — it provisions the rest into the vault. |
| 1Password CLI (`op`) | latest | Secrets resolve via `pass-cli inject` from the vault. `make env` is a no-op without it. |

A coding-agent host (Claude Code / Amp / Codex CLI / OpenCode) running this repo's `AGENTS.md` is recommended — it knows the Clerk bootstrap, vault setup, and gate-firing conventions cold.

## First run

```bash
git clone https://github.com/usezombie/usezombie.git ~/Projects/usezombie
cd ~/Projects/usezombie

# 1. Hydrate .env (zombied) from your Proton Pass / 1Password vault.
#    ENV=local pulls dev-machine defaults; ENV=dev|prod pull deploy targets.
make env ENV=local

# 2. Stand up Postgres + Redis + zombied (migrates the DB on first run).
make up

# 3. Frontend dashboard. Reads NEXT_PUBLIC_API_URL from ui/packages/app/.env.local
#    — point it at http://localhost:3000 (or whatever your zombied is bound to).
cd ui/packages/app
echo "NEXT_PUBLIC_API_URL=http://localhost:3000" > .env.local
bun install
bun run dev
```

There is no separate `.env.example` in `ui/packages/app/` because the only required value is `NEXT_PUBLIC_API_URL`; create `.env.local` by hand the first time. Marketing site (`ui/packages/website/`) needs no env for local dev.

## Verification cycle

```bash
make lint               # eslint + zig fmt + redocly + spec template audit
make test               # Tier 1 — Zig unit + zombiectl + website + app vitest
make test-integration   # Tier 2 — Zig vs real Postgres + Redis (run with services up)
```

`make test-integration` requires `make up` to have already provisioned Postgres + Redis. Run `make down && make up` first if you want a clean DB.

## CLI for non-prod backends

`zombiectl` defaults to `https://api.usezombie.com`. Three ways to point it at a local zombied:

| Scope | How |
|---|---|
| One command | `zombiectl --api http://localhost:3000 <command>` |
| Whole shell session | `export ZOMBIE_API_URL=http://localhost:3000` |
| Sticky per-install | `zombiectl login --api http://localhost:3000` (writes `~/.config/zombiectl/credentials.json`) |

Precedence: `--api` flag → `ZOMBIE_API_URL` → `API_URL` → saved credentials → default.

# Contributing

## Git hooks (run them, don't bypass them)

```bash
git config core.hooksPath .githooks
```

That wires up two hooks:

| Hook | What it runs | Source |
|---|---|---|
| Pre-commit | `gitleaks` + the doc-read / milestone-id / pub-surface audits | [`.githooks/pre-commit`](.githooks/pre-commit) |
| Pre-push | `make test` always; `make test-integration-stub` + `make test-integration` + `make memleak` when the push touches Zig | [`.githooks/pre-push`](.githooks/pre-push) |

`git push --no-verify` is documented as discouraged in `AGENTS.md` and exists only for emergencies — don't make it a habit.

## AGENTS.md and dotfiles

Every coding agent in this repo reads [`AGENTS.md`](AGENTS.md). That file is **a symlink** to [`~/Projects/dotfiles/AGENTS.md`](https://github.com/your-org/dotfiles) — Captain's opinionated cross-repo operating model. Without the symlink target, agents fall back to the on-disk copy, but you'll be out of sync with the global rules.

Bootstrap once per machine:

```bash
git clone <your dotfiles remote> ~/Projects/dotfiles
ln -sf ~/Projects/dotfiles/AGENTS.md ~/Projects/usezombie/AGENTS.md
```

Other things that live in `~/Projects/dotfiles/` and that agents in this repo expect:

- `~/Projects/dotfiles/skills/release-template.md` — canonical changelog template; `CHORE(close)` re-sources this on every release.
- `~/Projects/dotfiles/skills/*.md` — agent skill libraries, vault-resolution helpers, common playbook fragments.

Treat `~/Projects/dotfiles` as load-bearing for any cross-repo automation.

# Repos

| Repo | What it is |
|---|---|
| [usezombie/usezombie](https://github.com/usezombie/usezombie) | Control plane + worker + CLI (this repo) |
| [usezombie/docs](https://github.com/usezombie/docs) | User docs ([docs.usezombie.com](https://docs.usezombie.com)) |
| [usezombie/posthog-zig](https://github.com/usezombie/posthog-zig) | PostHog SDK for Zig |

MIT — Copyright (c) 2026 usezombie.
