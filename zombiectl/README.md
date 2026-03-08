# zombiectl

JavaScript CLI for UseZombie operator workflows.

## Install

```bash
npm install -g zombiectl
zombiectl --help
```

## Usage

Common flows (installed binary):

```bash
zombiectl login
zombiectl workspace add https://github.com/org/repo
zombiectl specs sync
zombiectl run
zombiectl run status <run_id>
zombiectl doctor --json
```

`workspace add` opens the UseZombie GitHub App install page and binds workspace via callback automatically.
Global flags:
- `--api <url>` API base URL (default `http://localhost:3000`)
- `--json` machine-readable output
- `--no-open` do not auto-open browser on login
- `--no-input` disable prompts (reserved for non-interactive flows)
- `--help`
- `--version`

## Verify

```bash
bun test
bun run build
```
