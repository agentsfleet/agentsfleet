<div align="center">

<img src="branding/usezombie-mark-glow.png" width="180" alt="usezombie" />

# Your deploy failed. The agent already knows why.

[![Get early access](https://img.shields.io/badge/usezombie-Get_early_access-5EEAD4?style=for-the-badge)](https://usezombie.com)
[![Docs](https://img.shields.io/badge/Docs-blue?style=for-the-badge)](https://docs.usezombie.com)
[![CI](https://github.com/usezombie/usezombie/actions/workflows/test.yml/badge.svg?branch=main)](https://github.com/usezombie/usezombie/actions/workflows/test.yml?query=branch%3Amain)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)

</div>

---

> **Early Access Preview.** APIs and CLI may change before GA.

A **Zombie** wakes on your events (webhook · cron · steer), gathers evidence against your infra, posts an evidenced diagnosis to Slack, and records every action in a replayable event log. Markdown-defined. Self-managed provider keys. Hosted on `api.usezombie.com`.

**Trying it as a user?** Skip the rest of this README and go straight to **[docs.usezombie.com/quickstart](https://docs.usezombie.com/quickstart)** — free to try, no card required, full install + first run in under five minutes. Current rates on [`usezombie.com/#pricing`](https://usezombie.com/#pricing).

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
| 1Password CLI (`op`) | latest | Secrets resolve via `pass-cli inject` from the vault. Required for the `.env` step in First Run. |

A coding-agent host (Claude Code / Amp / Codex CLI / OpenCode) running this repo's `AGENTS.md` is recommended — it knows the Clerk bootstrap, vault setup, and gate-firing conventions cold.

## First run

```bash
git clone https://github.com/usezombie/usezombie.git ~/Projects/usezombie
cd ~/Projects/usezombie

# 1. Hydrate .env (zombied) from your Proton Pass / 1Password vault.
#    Swap .env.local.tpl for .env.dev.tpl / .env.prod.tpl as needed.
pass-cli inject -i .env.local.tpl -o .env -f && chmod 600 .env

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
make lint-all           # zig fmt + zlint + oxlint + redocly + actionlint + schema/workflow gates
make test-unit-all      # Tier 1 — Zig units + multi-package coverage + agent-skill unit tests
make test-integration   # Tier 2 — Zig vs real Postgres + Redis (run with services up)
```

`make test-integration` requires `make up` to have already provisioned Postgres + Redis. Run `make down && make up` first if you want a clean DB.

## Running acceptance tests locally

The dashboard acceptance suite (`ui/packages/app/tests/e2e/acceptance/`) is a separate harness from `make test-integration`. It hits **live `api-dev.usezombie.com`** — not a local zombied — so you're not accidentally writing to prod, but the suite is **not pure-read** either: it provisions fixture-user tenants on api-dev and tears them down on success.

### Env vars the harness needs

`global-setup.ts` requires five env vars resolved from the org's 1Password DEV vault (`ZMB_CD_DEV`):

| Env var | Vault path |
|---|---|
| `NEXT_PUBLIC_API_URL` | hardcode `https://api-dev.usezombie.com` |
| `CLERK_SECRET_KEY` | `op://ZMB_CD_DEV/clerk-dev/secret-key` |
| `CLERK_PUBLISHABLE_KEY` | `op://ZMB_CD_DEV/clerk-dev/publishable-key` |
| `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY` | same value as `CLERK_PUBLISHABLE_KEY` |
| `CLERK_WEBHOOK_SECRET` | `op://ZMB_CD_DEV/clerk-dev/webhook-secret` |

### One-shell-line runner

```bash
cd ui/packages/app && \
  NEXT_PUBLIC_API_URL=https://api-dev.usezombie.com \
  CLERK_SECRET_KEY=$(op read 'op://ZMB_CD_DEV/clerk-dev/secret-key') \
  CLERK_PUBLISHABLE_KEY=$(op read 'op://ZMB_CD_DEV/clerk-dev/publishable-key') \
  NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY=$(op read 'op://ZMB_CD_DEV/clerk-dev/publishable-key') \
  CLERK_WEBHOOK_SECRET=$(op read 'op://ZMB_CD_DEV/clerk-dev/webhook-secret') \
  bun run test:acceptance
```

Run a single spec instead of the whole suite: `bun run test:acceptance -- <pattern>`.

### Why these env vars

The harness mints fixture-user JWTs **out-of-band via Clerk's Backend API** (per the two-token Clerk model — see [`docs/AUTH.md`](docs/AUTH.md)). The vault items resolve to the org's Clerk DEV instance: `CLERK_SECRET_KEY` calls the Backend API; the publishable keys are for the dashboard's runtime Clerk-SDK loading; the webhook secret is for identity-event ingestion.

### Side-effects + cleanup

- **Side-effect:** `global-setup` provisions two fixture-user tenants on api-dev (`regular@usezombie.dev` + `admin@usezombie.dev`).
- **Teardown:** `global-teardown` revokes them via the Clerk Backend API on success. On failure, they may linger — clean up via the Clerk DEV dashboard's user-management surface if you see drift.

## Unit tests + typecheck (inner loop)

For dashboard work that doesn't touch live api-dev:

```bash
cd ui/packages/app
bun run test          # vitest — components, helpers, libs
bun run typecheck     # tsc --noEmit (strict mode)
```

Same shape for `ui/packages/website/` and `ui/packages/design-system/`.

## Common test failures

Two failure modes the M74_002 milestone surfaced — documented here so the next contributor doesn't reverse-engineer them from a red CI log.

### `global-setup 404` on identity-events

**Symptom:** `POST /v1/webhooks/clerk` returns 404 during the harness's tenant bootstrap.

**Cause:** historical wrong path. The actual zombied endpoint is `/v1/auth/identity-events/clerk` (per `src/http/router.zig`). `/v1/webhooks/clerk` was retired.

**Fix:** confirm `global-setup.ts` bootstrap URL targets `/v1/auth/identity-events/clerk`. If it doesn't, update it.

### "Client Component SSR import boundary" error referencing `@clerk/nextjs/server`

**Symptom:** dev-server or test run fails with `You're importing a Client Component into a server file that imports server-only modules`, citing `@clerk/nextjs/server`.

**Cause:** somewhere in the import chain, a file with `"use client"` at the top is transitively reaching `@/lib/auth/server` (which imports `@clerk/nextjs/server`). Server-only imports can't be reached from client components.

**Fix:** grep upward from the offending import. Common culprit: a "helper" file imported by a client component statically imports a constant or type from `lib/auth/server.ts`. Move the shared shape to a `'use client'`-safe module, or restructure so the client component only imports from `lib/auth/client.ts`.

### Cross-reference

- The two-token Clerk model + how the harness mints fixture JWTs is in [`docs/AUTH.md`](docs/AUTH.md) (*Flow 2* + the *Test infrastructure — e2e fixture mint* section).
- Post-Stage-1 of the planned dashboard cleanup, the harness will mint via the customized session-token endpoint instead of the api template — see [`docs/AUTH.md`](docs/AUTH.md) *Roadmap — Flow 2 dashboard cleanup* section.

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
| Pre-commit | `gitleaks --staged` (always), then `make harness-verify` and the matching `lint-*` / `check-*` targets in parallel based on which surfaces are staged (`*.zig` → `lint-zig`, `ui/packages/website/*` → `lint-website`, `ui/packages/app/*` → `lint-app`, `ui/packages/design-system/*` → `lint-design-system` + `lint-app`, `zombiectl/*` → `lint-zombiectl`, `scripts/*.sh` → `lint-shell`, `schema/*.sql` → `check-schema-gate` + `lint-zig`, `public/openapi/*` → `check-openapi`, `.github/workflows/*` or `Makefile`/`make/*.mk` → `check-gh-actions-valid`). Pure-docs commits skip the lint pass entirely. | [`.githooks/pre-commit`](.githooks/pre-commit) |
| Pre-push | Surface-aware `test-unit-*` lanes in parallel (`*.zig` → `test-unit-zombied` — which internally chains `test-unit-executor`; `zombiectl/*` → `test-unit-zombiectl`; `ui/packages/website/*` → `test-unit-website`; `ui/packages/app/*` → `test-unit-app`; `ui/packages/design-system/*` → `test-unit-design-system` + `test-unit-app`; `tests/skill-evals/*` or `skills/*` → `test-unit-skills`). Pure-docs/pure-hook pushes run nothing. `test-integration` and `memleak` run in CI only — they no longer block pushes. | [`.githooks/pre-push`](.githooks/pre-push) |

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
