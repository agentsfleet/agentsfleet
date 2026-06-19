# agentsfleet

The official Command Line Interface (CLI) for [agentsfleet](https://agentsfleet.net).

[![Get early access](https://img.shields.io/badge/agentsfleet-Get_early_access-5EEAD4?style=for-the-badge)](https://agentsfleet.net)
[![Docs](https://img.shields.io/badge/Docs-blue?style=for-the-badge)](https://docs.agentsfleet.net)
[![npm](https://img.shields.io/npm/v/@agentsfleet/cli?style=for-the-badge&color=cb3837)](https://www.npmjs.com/package/@agentsfleet/cli)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)

Authenticate, manage workspaces, install agents, tail their events, and operate your agentsfleet deployment from the terminal.

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
agentsfleet workspace add my-workspace

# Verify configuration and connectivity
agentsfleet doctor
```

## Commands

### User

| Command | Description |
|---------|-------------|
| `login [--timeout-sec N] [--poll-ms N] [--no-open]` | Authenticate via browser |
| `logout` | Clear stored credentials |
| `workspace add [<name>]` | Create a new workspace |
| `workspace list` | List workspaces |
| `workspace use <workspace_id>` | Set the active workspace |
| `workspace show [--workspace-id ID]` | Show workspace details |
| `workspace credentials` | Open the credential vault |
| `workspace delete <workspace_id>` | Delete a workspace (irreversible) |
| `doctor` | Diagnose CLI configuration and connectivity |

### Agent keys

| Command | Description |
|---------|-------------|
| `agent add` | Mint an agent API key for the workspace |
| `agent list` | List agent API keys |
| `agent delete <key_id>` | Revoke an agent API key |

### Integration grants

| Command | Description |
|---------|-------------|
| `grant list` | List integration grants in the workspace |
| `grant delete <grant_id>` | Revoke an integration grant |

### Tenant provider

| Command | Description |
|---------|-------------|
| `tenant provider show` | Show the active provider config |
| `tenant provider add --credential <name>` | Use a self-managed credential |
| `tenant provider delete` | Reset to the platform default |

### Billing

| Command | Description |
|---------|-------------|
| `billing show` | Plan, balance, and usage snapshot |

### Agents

| Command | Description |
|---------|-------------|
| `install --from <path>` | Register an agent from `<path>` |
| `list [--cursor C] [--limit N]` | List agents (paginated) |
| `status [<agent_id>]` | Show agent status |
| `stop <agent_id>` | Halt the session (resumable) |
| `resume <agent_id>` | Resume from stopped |
| `kill <agent_id>` | Mark terminal (irreversible) |
| `delete <agent_id>` | Hard-delete (kill first) |
| `logs <agent_id>` | Tail agent activity |
| `events <agent_id> [opts]` | Page through historical events |
| `steer <agent_id> "<msg>"` | Send a message; stream response |

### Memory (read-only)

Inspect a agent's durable memory — newest-first, raw entries (the reader judges relevance; there is no ranking). A terminal gets an aligned table with content previews; piped or `--json` output is the server envelope verbatim with full content. Empty results exit 0.

| Command | Description |
|---------|-------------|
| `memory list --agent <id> [--category <name>] [--limit <n>]` | List entries newest-first |
| `memory search --agent <id> <query> [--limit <n>]` | Substring-search keys and content |

Both verbs accept `--workspace <id>` to override the active workspace. The server caps `--limit` at 100 (defaults: 100 for list, 20 for search). There are no write verbs — durable memory is written only by the runner plane.

### Workspace credentials

Workspace-scoped tool credentials live in the vault (Slack, GitHub, Fly, Upstash, etc.). Secret bytes are never echoed back.

| Command | Description |
|---------|-------------|
| `credential add <name> --data=@-` | Add a credential (pipe JSON on stdin; skip if exists) |
| `credential add <name> --data=@- --force` | Overwrite an existing credential |
| `credential add <name> --data='<json>'` | Add a credential (inline JSON, exposes secret to shell history) |
| `credential show <name>` | Check existence and `created_at` (never echoes secret) |
| `credential list` | List workspace credentials |
| `credential delete <name>` | Remove a workspace credential |

## Global flags

| Flag | Description |
|------|-------------|
| `--api <url>` | API base URL |
| `--json` | Machine-readable JSON output |
| `--no-input` | Disable interactive prompts |
| `--no-open` | Skip auto-opening browser on `login` |
| `--version` | Show version and exit |
| `--help`, `-h` | Show help text |

## Environment variables

| Variable | Description |
|----------|-------------|
| `AGENTSFLEET_API_URL` | API base URL (overridden by `--api`) |
| `AGENTSFLEET_API_KEY` | Service API key; overrides a stored `login` session |
| `AGENTSFLEET_STATE_DIR` | Override the config directory (default `~/.config/agentsfleet`) |
| `NO_COLOR` | Any non-empty value disables color |

## Configuration

| Item | Path |
|------|------|
| Credentials | `~/.config/agentsfleet/credentials.json` |
| Workspaces | `~/.config/agentsfleet/workspaces.json` |

Precedence for API base URL: `--api` flag → `AGENTSFLEET_API_URL` → saved credentials → default (`https://api.agentsfleet.net`).

## Links

- [Documentation](https://docs.agentsfleet.net)
- [Website](https://agentsfleet.net)
- [GitHub](https://github.com/agentsfleet/agentsfleet)
- [Discord](https://discord.gg/H9hH2nqQjh)

## License

MIT
