# agentsfleet

The official Command Line Interface (CLI) for [agentsfleet](https://agentsfleet.net).

[![Get early access](https://img.shields.io/badge/agentsfleet-Get_early_access-B8F0CE?style=for-the-badge&labelColor=151716)](https://agentsfleet.net/#pricing)
[![Docs](https://img.shields.io/badge/Docs-B8F0CE?style=for-the-badge)](https://docs.agentsfleet.net)
[![npm](https://img.shields.io/npm/v/@agentsfleet/cli?style=for-the-badge&color=cb3837)](https://www.npmjs.com/package/@agentsfleet/cli)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)

Sign in, manage workspaces, start a fleet with its first message, keep talking to it, and inspect its activity from your terminal.

> **Pre-release** — agentsfleet is in pre-release. The Application Programming
> Interface (API), the CLI, and behavior may change without notice before
> General Availability (GA).

## Install

**Package release pending.** The public npm package is not available yet. [Build the CLI from source](https://github.com/agentsfleet/agentsfleet#quick-start) while the release is prepared.

After the package is published, install it with:

```bash
npm install -g @agentsfleet/cli
```

Requires [Bun](https://bun.sh) ≥ 1.4.5. The CLI runs on Bun.

## Quick start

```bash
# Authenticate with your agentsfleet account (opens browser)
agentsfleet login

# Create a workspace
agentsfleet workspace create my-workspace

# Verify configuration and connectivity
agentsfleet doctor

# Start a fleet with your first message; it prints the fleet's id
agentsfleet fleet create "Watch checkout errors and tell me when they spike"

# Keep talking to it, by id or by @name; with no message it opens a prompt
agentsfleet fleet steer <fleet_id> "check it now"
```

A fleet asks for a secret or an integration when it needs one;
`agentsfleet fleet approvals list` shows what waits for your answer.

## Usage

The full command reference lives at **[docs.agentsfleet.net](https://docs.agentsfleet.net)**
and is versioned with each release:

| Page | Covers |
|------|--------|
| [Install](https://docs.agentsfleet.net/cli/install) | Install, upgrade, supported runtimes |
| [Commands](https://docs.agentsfleet.net/cli/agentsfleet) | Every command and its flags |
| [Platform administration](https://docs.agentsfleet.net/cli/platform) | Runner, model, and provider commands; required operator permissions |
| [Global flags](https://docs.agentsfleet.net/cli/flags) | `--api`, `--json` — and where the per-command flags live |
| [Configuration](https://docs.agentsfleet.net/cli/configuration) | Environment variables, config paths, precedence |

`agentsfleet --help` and `agentsfleet <command> --help` print the same surface
from the binary you actually have installed.

Run `agentsfleet pet` to play Orbit, a local paddle game. Press Space to serve,
and move with the arrows or A/D. Aim with the edges of the paddle, collect
stars for bonus points, and build a streak as the court speeds up. You get
three lives. Press R to restart with your session best or Q to quit.

The request loader shows a bouncing ball. Set `AGENTSFLEET_REDUCED_MOTION=1` for a static status.

`agentsfleet pet --json` returns `{"name":"Orbit"}`. The game runs locally without signing in.

## Development

Building, testing, and the repository layout: [`docs/development.md`](https://github.com/agentsfleet/agentsfleet/blob/main/docs/development.md).
Contribution workflow for this package: [`cli/CONTRIBUTING.md`](https://github.com/agentsfleet/agentsfleet/blob/main/cli/CONTRIBUTING.md).

## Links

- [Documentation](https://docs.agentsfleet.net)
- [Website](https://agentsfleet.net)
- [GitHub](https://github.com/agentsfleet/agentsfleet)
- [Discord](https://discord.gg/H9hH2nqQjh)

## License

MIT
