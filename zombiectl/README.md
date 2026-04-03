# zombiectl

The official CLI for [UseZombie](https://usezombie.com).

Manage workspaces, trigger runs, monitor agents, and operate your UseZombie deployment from the terminal.

## Install

```bash
npm install -g @usezombie/zombiectl
```

## Quick start

```bash
# Authenticate with your UseZombie account
zombiectl login

# Add a GitHub repository as a workspace
zombiectl workspace add https://github.com/org/repo

# Sync specs and trigger a run
zombiectl specs sync
zombiectl run

# Check your environment
zombiectl doctor
```

## Features

- **Workspaces** — add, switch, and manage GitHub-connected workspaces
- **Runs** — trigger, list, and cancel agent runs
- **Specs** — sync and initialize specifications
- **Agent operations** — view profiles, improvement reports, and proposals
- **Diagnostics** — `doctor` command validates your environment
- **JSON output** — `--json` flag for scripts and CI/CD pipelines

## Commands

| Command | Description |
|---------|-------------|
| `login` | Authenticate with UseZombie |
| `logout` | Clear stored credentials |
| `workspace add <url>` | Connect a GitHub repository |
| `specs sync` | Sync specifications from the API |
| `spec init` | Initialize a new spec |
| `run` | Trigger a run |
| `runs list` | List run history |
| `runs cancel <id>` | Cancel an in-flight run |
| `agent profile <id>` | View agent profile |
| `agent improvement-report <id>` | View improvement report |
| `agent proposals <id>` | List agent proposals |
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
