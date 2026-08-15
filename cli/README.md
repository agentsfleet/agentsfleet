# agentsfleet

The official Command Line Interface (CLI) for [agentsfleet](https://agentsfleet.net).

[![Get early access](https://img.shields.io/badge/agentsfleet-Get_early_access-5EEAD4?style=for-the-badge)](https://agentsfleet.net)
[![Docs](https://img.shields.io/badge/Docs-blue?style=for-the-badge)](https://docs.agentsfleet.net)
[![npm](https://img.shields.io/npm/v/@agentsfleet/cli?style=for-the-badge&color=cb3837)](https://www.npmjs.com/package/@agentsfleet/cli)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)

Authenticate, manage workspaces, install Fleets, tail their events, and operate your agentsfleet deployment from the terminal.

> **Pre-release** — agentsfleet is in pre-release. Application Programming Interface (API), CLI, and behavior may change without notice before General Availability (GA). This package is published under the `next` dist-tag.

## Install

```bash
npm install -g @agentsfleet/cli@next
```

Requires Node.js ≥ 24 (or Bun ≥ 1.3).

## Quick start

```bash
# Authenticate with your agentsfleet account (opens browser)
agentsfleet login

# Create a workspace
agentsfleet workspace create my-workspace

# Verify configuration and connectivity
agentsfleet doctor
```

## Usage

The full command reference lives at **[docs.agentsfleet.net](https://docs.agentsfleet.net)**
and is versioned with each release:

| Page | Covers |
|------|--------|
| [Install](https://docs.agentsfleet.net/cli/install) | Install, upgrade, supported runtimes |
| [Commands](https://docs.agentsfleet.net/cli/agentsfleet) | Every command and its flags |
| [Global flags](https://docs.agentsfleet.net/cli/flags) | `--api`, `--json`, `--no-input`, `--no-open` |
| [Configuration](https://docs.agentsfleet.net/cli/configuration) | Environment variables, config paths, precedence |

`agentsfleet --help` and `agentsfleet <command> --help` print the same surface
from the binary you actually have installed.

This file deliberately carries no command tables. It used to mirror all four
pages above, which made every flag change two edits — and the copy that drifted
was the one shipped to npm, where nobody could see the original to compare.

## Development

Building, testing, and the repository layout: [`docs/DEVELOPMENT.md`](https://github.com/agentsfleet/agentsfleet/blob/main/docs/DEVELOPMENT.md).
Contribution workflow for this package: [`cli/CONTRIBUTING.md`](https://github.com/agentsfleet/agentsfleet/blob/main/cli/CONTRIBUTING.md).

## Links

- [Documentation](https://docs.agentsfleet.net)
- [Website](https://agentsfleet.net)
- [GitHub](https://github.com/agentsfleet/agentsfleet)
- [Discord](https://discord.gg/H9hH2nqQjh)

## License

MIT
