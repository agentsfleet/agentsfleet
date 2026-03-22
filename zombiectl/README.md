# zombiectl

JavaScript CLI for UseZombie operator workflows.

Local state lives in `~/.config/zombiectl/` by default:
- `credentials.json` — auth/session state
- `workspaces.json` — current workspace selection and local cache
- `ZOMBIE_STATE_DIR` overrides the base directory for tests or custom automation

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

Operator trajectory flow:

```bash
zombiectl agent profile <agent-id>
zombiectl agent improvement-report <agent-id>
zombiectl agent proposals <agent-id>
zombiectl agent proposals <agent-id> veto <proposal-id> --reason "operator pause"
```

`workspace add` opens the UseZombie GitHub App install page and binds workspace via callback automatically.
Global flags:
- `--api <url>` API base URL (default `http://localhost:3000`)
- `--json` machine-readable output
- `--no-open` do not auto-open browser on login
- `--no-input` disable prompts (reserved for non-interactive flows)
- `--help`
- `--version`

Analytics env vars (optional):
- `ZOMBIE_POSTHOG_KEY` override the bundled PostHog project API key (`phc_...`) for local/dev testing
- `ZOMBIE_POSTHOG_ENABLED` set `false`/`0` to disable telemetry even when the bundled key exists
- `ZOMBIE_POSTHOG_HOST` override PostHog host (default `https://us.i.posthog.com`)

Standard operator path:

```bash
# DEV
export ZOMBIE_POSTHOG_KEY="$(op read 'op://ZMB_CD_DEV/posthog-dev/credential')"

# PROD
export ZOMBIE_POSTHOG_KEY="$(op read 'op://ZMB_CD_PROD/posthog-prod/credential')"
```

This follows the same milestone playbook and `scripts/check-credentials.sh` contract as the other deploy/runtime keys.

Analytics key policy:
- `zombiectl` ships with a bundled default PostHog project key so end users do not need analytics setup after `npm install -g`.
- The bundled key is not an auth secret; it is a write-scoped ingestion key.
- Do not persist analytics keys in CLI auth/session state such as `credentials.json`.
- If exposed, the expected risk is analytics pollution or noisy metrics, not control-plane access.

## Verify

```bash
bun test
bun run build
```
