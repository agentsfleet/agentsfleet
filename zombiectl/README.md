# zombiectl

The official CLI for [UseZombie](https://usezombie.com).

Install zombies, manage workspaces, monitor zombie events, and operate your UseZombie deployment from the terminal.

> **Pre-release** — UseZombie is in pre-release. APIs, CLI, and behavior may change without notice before general availability. This package is published under the `next` dist-tag.

## Install

```bash
npm install -g @usezombie/zombiectl@next
```

## Quick start

```bash
# Authenticate with your UseZombie account
zombiectl login

# Add a GitHub repository as a workspace
zombiectl workspace add https://github.com/org/repo

# Check your environment
zombiectl doctor
```

## Features

- **Workspaces** — add, switch, and manage GitHub-connected workspaces
- **Zombies** — install, list, kill, and steer zombies; tail their event streams
- **Credential vault** — store workspace-scoped tool credentials (Slack, GitHub, Fly, Upstash, etc.)
- **Diagnostics** — `doctor` command validates your environment
- **JSON output** — `--json` flag for scripts and CI/CD pipelines

## Commands

| Command | Description |
|---------|-------------|
| `login` | Authenticate with UseZombie |
| `logout` | Clear stored credentials |
| `workspace add <url>` | Connect a GitHub repository |
| `workspace list` | List your workspaces |
| `workspace show` | Show details for the active workspace |
| `workspace use <id>` | Switch the active workspace |
| `workspace delete <id>` | Remove a workspace from your local list |
| `install --from <path>` | Install a zombie from a local template directory |
| `list` | List zombies in the active workspace |
| `status [<zombie_id>]` | Show zombie status |
| `kill <zombie_id>` | Delete a zombie |
| `logs <zombie_id>` | Tail zombie activity |
| `steer <zombie_id> <message>` | Send a message into a zombie's loop |
| `events <zombie_id>` | Stream zombie events (SSE) |
| `credential add <name> --data '<json>'` | Add a workspace credential (JSON object) |
| `credential list` | List workspace credentials (no secret bytes) |
| `credential delete <name>` | Remove a workspace credential |
| `doctor` | Run environment diagnostics |
| `doctor --json` | Diagnostics in JSON format |

## Global flags

| Flag | Description |
|------|-------------|
| `--api <url>` | API base URL |
| `--json` | Machine-readable JSON output |
| `--no-open` | Do not auto-open browser on login |
| `--no-input` | Disable interactive prompts |
| `--version` | Print version and exit |
| `--help` | Show help text |

## Configuration

| Item | Path |
|------|------|
| Credentials | `~/.config/zombiectl/credentials.json` |
| Workspaces | `~/.config/zombiectl/workspaces.json` |

Override the config directory with `ZOMBIE_STATE_DIR`.

## Links

- [Documentation](https://docs.usezombie.com)
- [Website](https://usezombie.com)
- [GitHub](https://github.com/usezombie/usezombie)
- [Discord](https://discord.gg/H9hH2nqQjh)

## License

MIT
