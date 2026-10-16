<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="branding/agentsfleet-dark.svg" />
  <source media="(prefers-color-scheme: light)" srcset="branding/agentsfleet-light.svg" />
  <img src="branding/agentsfleet-light.svg" width="280" alt="agentsfleet" />
</picture>

# AI agents that wake when production breaks.

[![CI](https://img.shields.io/github/actions/workflow/status/agentsfleet/agentsfleet/test.yml?branch=main&label=CI&logo=github&logoColor=white)](https://github.com/agentsfleet/agentsfleet/actions/workflows/test.yml?query=branch%3Amain)
[![rust-afd coverage](https://img.shields.io/codecov/c/github/agentsfleet/agentsfleet?flag=rust-afd&label=rust-afd&logo=codecov&logoColor=white)](https://codecov.io/gh/agentsfleet/agentsfleet?flags[0]=rust-afd)
[![app coverage](https://img.shields.io/codecov/c/github/agentsfleet/agentsfleet?flag=app&label=app&logo=codecov&logoColor=white)](https://codecov.io/gh/agentsfleet/agentsfleet?flags[0]=app)
[![website coverage](https://img.shields.io/codecov/c/github/agentsfleet/agentsfleet?flag=website&label=website&logo=codecov&logoColor=white)](https://codecov.io/gh/agentsfleet/agentsfleet?flags[0]=website)
[![cli coverage](https://img.shields.io/codecov/c/github/agentsfleet/agentsfleet?flag=cli&label=cli&logo=codecov&logoColor=white)](https://codecov.io/gh/agentsfleet/agentsfleet?flags[0]=cli)
[![Rust](https://img.shields.io/badge/Rust-1.99.0-CE422B?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Docs](https://img.shields.io/badge/Docs-agentsfleet.net-B8F0CE?labelColor=151716&logo=gitbook&logoColor=B8F0CE)](https://docs.agentsfleet.net)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

</div>

**[agentsfleet](https://agentsfleet.net)** is an open-source runtime for agents configured for recurring jobs. Each fleet has instructions, tools, triggers, access rules, and a budget. Events or messages wake the fleet; runners investigate with its permitted tools and save the results.

- **Controlled access** — tools and network rules determine which actions a fleet can perform.
- **Durable results** — event history records requests, outcomes, and reports; live activity shows work in progress.
- **Model choice** — use the platform model or an account model configured with your own provider key.
- **Separate working instances** — each fleet keeps its own history and memory.

The current runtime imports Markdown playbooks from the fleet library. Chat authoring and a typed saved definition are [proposed for review](docs/architecture/chat_authoring.md); they are not available yet.

---

## Quick start

Already invited? [Sign in to the app](https://app.agentsfleet.net/sign-in). To request access, visit [Early access](https://agentsfleet.net/#pricing).

The public command-line interface (CLI) package is waiting for release. Build the CLI from this repository with [Bun 1.4 or newer](https://bun.sh):

```bash
git clone https://github.com/agentsfleet/agentsfleet.git
cd agentsfleet/cli
bun install
bun run build
bun ./dist/bin/agentsfleet.js login
```

Sign in in the browser, then enter the browser's verification code in the terminal. The CLI reports `login complete` after verification.

See the [CLI install guide](https://docs.agentsfleet.net/cli/install) for installer and release instructions. The [quickstart](https://docs.agentsfleet.net/quickstart) covers defining an agent in Markdown and connecting its tools and triggers.

---

## What's in this repo

| Directory | What |
|---|---|
| `rustd/` | Rust workspace — the `agentsfleetd` control plane (`afd_*` crates, one per plane) and the `agentsfleet-runner` execution daemon (`afr_*` crates). |
| `ui/packages/app/` | Dashboard — Next.js, Clerk auth |
| `ui/packages/website/` | Marketing site — [agentsfleet.net](https://agentsfleet.net) |
| `ui/packages/design-system/` | Shared UI components |
| `cli/` | Command-line interface (CLI) — install, manage agents, tail runs |
| `public/openapi.json` | OpenAPI reference generated from the daemon's route and schema definitions |
| `schema/` | Postgres migrations |
| `playbooks/` | First setup, deployment, verification, and operations |

---

## Local development

**Prerequisites:** [Rust 1.99.0](https://rustup.rs) (pinned by `rustd/rust-toolchain.toml`, which rustup installs on the first `cargo` run inside `rustd/`) · [Docker](https://www.docker.com) (Postgres, Dragonfly, QStash) · [Bun ≥1.4](https://bun.sh) · [Clerk](https://clerk.com) dev project · [1Password CLI](https://1password.com/downloads/command-line/) for secrets

```bash
git clone https://github.com/agentsfleet/agentsfleet.git
cd agentsfleet

# Run provision-env-1password first. See docs/development.md for shared-worktree setup.
make up           # Shared services + this worktree's native daemon; migrate before serving

# Install in this order. The root install alone does NOT reach the workspace
# packages, and the coverage lane then fails resolving `next/headers` — a
# failure that reads like a code defect and is not one.
bun install
(cd cli && bun install && bun run build)
for pkg in ui/packages/*/; do (cd "$pkg" && bun install); done

export NEXT_PUBLIC_API_URL="$(bash scripts/local-dev.sh url)"
cd ui/packages/app && bun run dev
```

Run `provision-env-1password` before local tests. The shell sources
`~/.config/agentsfleet/.env`, which names the UI, runner, and daemon files under
that directory through `AGENTSFLEET_UI_ENV_FILE`, `AGENTSFLEET_RUNNER_ENV_FILE`,
and `AGENTSFLEET_RUSTD_ENV_FILE`. Local UI and daemon tests read their files
directly; the runner path is available to tests that need it. No worktree
environment-file symlinks are needed. The app refuses to guess a backend when
`NEXT_PUBLIC_API_URL` is unset.

See [shared development worktrees](docs/development.md#shared-development-worktrees)
for startup, status, ownership and teardown.

**Verify:**

```bash
make lint-all
make test-unit-all
make test-integration-rustd   # needs Docker: live Postgres + Dragonfly
```

---

## CLI

`agentsfleet` defaults to **production**. From the repository root, select this worktree's URL with `--api` or the environment. Replace `<COMMAND>` with your command.

```bash
agentsfleet --api "$(bash scripts/local-dev.sh url)" <COMMAND>
export AGENTSFLEET_API_URL="$(bash scripts/local-dev.sh url)"
```

---

## Contributing

```bash
git config core.hooksPath .githooks
```

Project facts live in [`AGENTS.md`](AGENTS.md). The operating model and the
deterministic gate scripts are committed alongside them —
[`.orly/AGENTS.md`](.orly/AGENTS.md), `.orly/dispatch/`, and `.orly/audits/` — materialised
by [orly](https://github.com/agentsfleet/orly) and recorded in
`.orly/orly.json`. A clone carries its own rules; nothing resolves out of a
developer's home directory.

Install orly at the version this repository records, so a local run and
Continuous Integration grade against the same rules:

```bash
npm install -g "@agentsfleet/orly@$(jq -r .orly_version .orly/orly.json)"

orly doctor                     # report drift between the lock and the tree
orly update --no-hooks          # re-materialise at the installed version
orly gate work                  # what the pre-commit hook runs
```

`--no-hooks` is not optional here: this repository owns `.githooks/pre-commit`
and `.githooks/pre-push`, and orly declines to overwrite hooks it did not
write.

`make harness-verify` runs shared gates from `.orly/audits/` and repository-owned checks from `audits/`.

Read about [architecture](docs/architecture/), start with the
[operator playbooks](playbooks/README.md), or jump into
[local development](#local-development).

---

MIT — Copyright (c) 2026 agentsfleet.
