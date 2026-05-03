# Reference — credential resolution

The install-skill resolves four tool credentials at install time:
`fly`, `slack`, `github`, optional `upstash`. Each credential is an
opaque JSON object stored in the workspace vault under a name that the
M43 webhook ingest and the executor's `${secrets.NAME.FIELD}`
substitution path both look up by convention.

## The credential JSON shapes

| Credential | Vault name | JSON body shape | Notes |
|---|---|---|---|
| GitHub | `github` (workspace-scoped) | `{"webhook_secret": "<base64-32>", "api_token": "<gh PAT>"}` | The skill generates `webhook_secret` only on first install for the workspace; on second install it prompts reuse-vs-scope. `api_token` is always resolved per-install via op → env → prompt. |
| Fly | `fly` | `{"api_token": "<fly PAT>", "host": "api.machines.dev"}` | `host` defaults to `api.machines.dev`; override only when the operator has a non-default Fly endpoint. |
| Slack | `slack` | `{"api_token": "<xoxb-bot-token>"}` | Single field. The bot must already be invited to the channel the operator named at install. |
| Upstash | `upstash` (optional) | `{"redis_url": "<value>", "redis_token": "<value>"}` | Skipped if the repo has no Redis evidence (no `upstash` strings in deploy config, no `REDIS_URL` env). |

The vault credential name is a **convention**, not a per-zombie
pointer. The webhook ingest resolver looks the credential up by
`name = trigger.source` automatically — so the generated TRIGGER.md
does not write a `signature.secret_ref:` field for the convention case.
The only exception is when the user picked per-zombie scoping (option B
in the install-skill's step 5): then the generated TRIGGER.md carries
`credential_name: github-{zombie_slug}` and that name overrides the
default lookup.

## Resolution order, per field

For each field of each credential JSON body, resolve in this order and
stop at the first hit:

1. **`op read`** — the user's existing 1Password layout. Example:

   ```bash
   op read 'op://Personal/fly-platform/api_token'
   op read 'op://Engineering/github-pats/installer-token'
   ```

   The skill does not prescribe a vault or item-naming convention.
   Each user has their own `op` layout. If the field returns empty or
   `op` errors out, fall through to step 2.

2. **Environment variable** named `ZOMBIE_CRED_<NAME>_<FIELD>` —
   uppercase, underscores. Examples:

   ```bash
   ZOMBIE_CRED_FLY_API_TOKEN
   ZOMBIE_CRED_FLY_HOST
   ZOMBIE_CRED_GITHUB_API_TOKEN
   ZOMBIE_CRED_SLACK_API_TOKEN
   ZOMBIE_CRED_UPSTASH_REDIS_URL
   ZOMBIE_CRED_UPSTASH_REDIS_TOKEN
   ```

   Useful in CI fixtures and for quick tests; not the recommended
   long-term home for production tokens.

3. **Masked interactive prompt** via the host's question primitive
   (or inline prompt when the host has none). Always mask the
   echo — these are secrets.

The `webhook_secret` field on the `github` credential is the one
exception: the skill generates it locally (32 CSPRNG bytes,
base64-encoded) on first install rather than reading it from any of
the three sources above. Subsequent installs reuse or scope-per-zombie
per the install-skill's step 5.

## Why JSON on stdin (`--data @-`)

The skill never passes credential JSON via `--data '<JSON>'` because:

- Shell history captures the full command line including the JSON.
- `ps`, `procfs`, and process-listing tools see the argv on Linux/Mac.
- Audit logs of the user's shell sessions get a verbatim copy.

Piping on stdin (`--data @-`) keeps secret bytes inside the process
boundary. The JSON arrives at `zombiectl` via `read(0)`; the parent
shell has no record of the payload.

## Storing fewer fields than the JSON shape suggests

`zombiectl credential add` accepts any JSON object — the field set is
not validated against a schema. If a credential body is missing a
field that downstream code needs (e.g. `slack` without `api_token`),
the failure surfaces at *use* time as `secret_not_found` against
`${secrets.slack.api_token}`, not at install time. The skill's
prompt-on-empty step prevents this in the happy path; manual edits
of the credential after install bypass it.

## Rotation

The skill does not rotate credentials on its own. The documented
rotation path is:

```bash
op read 'op://<vault>/<item>/api_token' \
  | jq -Rn '{webhook_secret: env.OLD_SECRET, api_token: input}' \
  | zombiectl credential add github --force --data @-
```

`--force` overrides the default skip-if-exists. The skill never emits
`--force` automatically — rotation is always a deliberate operator
action.
