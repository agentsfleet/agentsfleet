# zombiectl

The official Command Line Interface (CLI) for [usezombie](https://usezombie.com).

[![Try for free](https://img.shields.io/badge/usezombie-Try_for_free-5EEAD4?style=for-the-badge)](https://usezombie.com)
[![Docs](https://img.shields.io/badge/Docs-blue?style=for-the-badge)](https://docs.usezombie.com)
[![npm](https://img.shields.io/npm/v/@usezombie/zombiectl?style=for-the-badge&color=cb3837)](https://www.npmjs.com/package/@usezombie/zombiectl)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)

Authenticate, manage workspaces, install zombies, tail their events, and operate your usezombie deployment from the terminal.

> **Pre-release** — usezombie is in pre-release. Application Programming Interface (API), CLI, and behavior may change without notice before General Availability (GA). This package is published under the `next` dist-tag.

## Install

```bash
npm install -g @usezombie/zombiectl@next
```

Requires Node.js ≥ 24 (or Bun ≥ 1.3).

## Quick start

```bash
# Authenticate with your usezombie account (opens browser)
zombiectl login

# Create a workspace
zombiectl workspace add my-workspace

# Verify configuration and connectivity
zombiectl doctor
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

### Zombies

| Command | Description |
|---------|-------------|
| `install --from <path>` | Register a zombie from `<path>` |
| `list [--cursor C] [--limit N]` | List zombies (paginated) |
| `status [<zombie_id>]` | Show zombie status |
| `stop <zombie_id>` | Halt the session (resumable) |
| `resume <zombie_id>` | Resume from stopped |
| `kill <zombie_id>` | Mark terminal (irreversible) |
| `delete <zombie_id>` | Hard-delete (kill first) |
| `logs <zombie_id>` | Tail zombie activity |
| `events <zombie_id> [opts]` | Page through historical events |
| `steer <zombie_id> "<msg>"` | Send a message; stream response |

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
| `ZOMBIE_API_URL` | API base URL (overridden by `--api`) |
| `ZOMBIE_TOKEN` | Auth token (overridden by `login`) |
| `ZOMBIE_API_KEY` | API key for service auth |
| `ZOMBIE_STATE_DIR` | Override the config directory (default `~/.config/zombiectl`) |
| `NO_COLOR` | Any non-empty value disables color |

## Configuration

| Item | Path |
|------|------|
| Credentials | `~/.config/zombiectl/credentials.json` |
| Workspaces | `~/.config/zombiectl/workspaces.json` |

Precedence for API base URL: `--api` flag → `ZOMBIE_API_URL` → saved credentials → default (`https://api.usezombie.com`).

## Links

- [Documentation](https://docs.usezombie.com)
- [Website](https://usezombie.com)
- [GitHub](https://github.com/usezombie/usezombie)
- [Discord](https://discord.gg/H9hH2nqQjh)

## License

MIT
