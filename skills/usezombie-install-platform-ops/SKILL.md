---
name: usezombie-install-platform-ops
description: >
  Install a usezombie platform-ops zombie on this repo — watches GitHub
  Actions CD failures and posts evidenced diagnoses to Slack. Always load
  this skill before running `zombiectl zombie install` for platform-ops; it
  knows the doctor preflight, credential resolution order, webhook setup,
  and smoke-test steps that prevent silent failures.
license: Apache-2.0
metadata:
  author: usezombie
  version: "0.1.0"
  homepage: https://usezombie.com/docs/skills
  source: https://github.com/usezombie/usezombie
  requires:
    bins: [zombiectl, openssl, curl]
    optional_bins: [op]
inputs:
  - name: slack_channel
    description: Slack channel for diagnoses (e.g. "#platform-ops"). Required.
    required: true
  - name: prod_branch_glob
    description: Branch glob counted as production. Default "main".
    required: false
  - name: cron_schedule
    description: Cron expression for periodic health check (e.g. "*/30 * * * *"). Blank for none.
    required: false
references:
  - references/credential-resolution.md
  - references/failure-modes.md
  - references/byok-handoff.md
---

# usezombie-install-platform-ops

## Installation

Before invoking this skill, the user needs `zombiectl` on their PATH and
the `/usezombie-*` skills symlinked into the host's skill directory. Walk
the user through this once on a cold machine:

```bash
zombiectl --version
```

If the binary is not found:

```bash
npm install -g @usezombie/zombiectl
```

If `/usezombie-install-platform-ops` does not show up as a slash-command
in the host:

```bash
npx skills add usezombie/usezombie
```

Manual symlink fallback (when `npx skills` is unavailable or the host is
not in the registry):

```bash
ln -s "$(npm root -g)/@usezombie/zombiectl/skills/usezombie-install-platform-ops" \
  ~/.claude/skills/usezombie-install-platform-ops
```

Same shape for `~/.codex/skills/`, `~/.amp/skills/`, `~/.opencode/skills/`.

## Agent Protocol

This skill drives `zombiectl` non-interactively. Every `zombiectl`
invocation uses `--json` where the flag is supported, parses the JSON
response, and surfaces stderr verbatim on failure. Exit `0` is success;
non-zero is a hard stop. Never proceed past a failed step. Never silently
retry a failed step — surface it and let the user decide.

This skill is host-neutral. Resolve variables via the host's question
primitive when one exists (e.g. Claude Code's `AskUserQuestion`), or fall
back to inline natural-language prompts in the chat surface. **Do not
hard-code any one host's primitive in this skill body** — the same
`SKILL.md` runs in Claude Code, Amp, Codex CLI, and OpenCode.

## Authentication

`zombiectl` auth is checked once via `zombiectl doctor --json`. If
`auth_token_present` is `false`, print `Run zombiectl auth login first`
and stop. The skill never logs in on the user's behalf.

Tool credentials (`fly`, `slack`, `github`, `upstash`) resolve in this
order, per field:

1. `op read 'op://<your-vault>/<your-item>/<field>'` — uses the user's
   existing 1Password layout. The skill does not prescribe a vault or
   item-naming convention.
2. Environment variable `ZOMBIE_CRED_<NAME>_<FIELD>` (e.g.
   `ZOMBIE_CRED_FLY_API_TOKEN`).
3. Masked interactive prompt (host-neutral question primitive).

JSON bodies are piped through stdin into
`zombiectl credential add <name> --data @-` so secret bytes never appear
in shell history or process argv. Never pass JSON via `--data '<JSON>'`.

See [`references/credential-resolution.md`](references/credential-resolution.md)
for the full resolution table and op layout examples.

## Plan

Walk these twelve steps top-to-bottom. Stop on the first failure;
surface the diagnostic and let the user fix it before retrying.

1. **Doctor preflight.** Run `zombiectl doctor --json`. If any check
   fails — auth missing, no workspace binding, vault unreachable —
   surface the response and stop. The `tenant_provider` block in the
   response is the source of `model` and `context_cap_tokens` for the
   generated frontmatter; capture it.
2. **Read provider posture.** From doctor's `tenant_provider` block,
   note `mode` (`platform` or `byok`), `model`, and
   `context_cap_tokens`. Under `platform`, both values are real
   (e.g. `accounts/fireworks/models/kimi-k2.6` + `256000`); under
   `byok` you will write the visible sentinels `""` and `0` so the
   worker overlays from `core.tenant_providers` at trigger time.
3. **Detect the repo.** Read `.github/workflows/*.yml`, `fly.toml`,
   `Dockerfile`, `pyproject.toml`, `package.json` from the user's CWD.
   Infer the deploy target (Fly is the v1 happy path; "GitHub Actions
   only" is the v1 stop). If no `.github/workflows/` directory, stop
   with `GitHub Actions only in v1` and exit cleanly.
4. **Resolve three variables** via the host's question primitive or
   inline prompts: `slack_channel` (required, e.g. `#platform-ops`),
   `prod_branch_glob` (default `main`), `cron_schedule` (optional,
   e.g. `*/30 * * * *`; empty string disables the recurring check).
   Never ask about LLM model or BYOK — those come from doctor (step 2)
   and M48 respectively.
5. **Resolve the GitHub webhook secret.** Check whether the workspace
   already has a `github` credential with a `webhook_secret` field
   (`zombiectl credential show github --json` returns presence without
   echoing the secret bytes). Two paths:
   - **No existing secret (first install for the workspace):** generate
     32 CSPRNG bytes, base64-encode, hold in a local variable for the
     post-install summary. You will display it once, never log it,
     never write it to a file.
   - **Existing secret (second install):** ask the user to choose:
     **A) Reuse the workspace-shared secret** — write nothing new to
     `webhook_secret`; `api_token` still upserts. **B) Scope a
     per-zombie credential** — generate a new secret, store under
     credential name `github-{zombie_slug}`, and write
     `credential_name: github-{zombie_slug}` into the generated
     TRIGGER.md as the M43 override.
6. **Resolve four tool credentials.** For each of `fly`, `slack`,
   `github`, optional `upstash`, walk the resolution order
   (op → env → prompt) per field, build the JSON body, and pipe it
   into `zombiectl credential add <name> --data @-`. Default behaviour
   is skip-if-exists; the skill relies on this to avoid clobbering a
   workspace-shared `github.webhook_secret` on a second install. Never
   pass `--force` unless the user explicitly asked to rotate.
7. **Read the canonical template** from
   `~/.config/usezombie/samples/platform-ops/`. If the directory is
   missing, the npm install was corrupted or postinstall was skipped —
   print:

   ```
   Cannot find platform-ops template at ~/.config/usezombie/samples/platform-ops/.
   Reinstall: npm install -g @usezombie/zombiectl
   ```

   and exit. Never fetch from a URL. Never cache. The npm package
   version *is* the template version.
8. **Generate the per-repo zombie files.** Substitute the five
   placeholders into `~/.config/usezombie/samples/platform-ops/SKILL.md`
   and `TRIGGER.md`, then write the output to
   `.usezombie/platform-ops/` in the user's CWD:

   | Placeholder | Source |
   |---|---|
   | `{{slack_channel}}` | step 4 |
   | `{{prod_branch_glob}}` | step 4 (default `main` if blank) |
   | `{{cron_schedule}}` | step 4 (empty omits the cron-related prose) |
   | `{{model}}` | doctor `tenant_provider.model` (real value or `""` under BYOK) |
   | `{{context_cap_tokens}}` | doctor `tenant_provider.context_cap_tokens` (real value or `0` under BYOK) |

   If `.usezombie/platform-ops/` already exists, prompt overwrite
   (default `N`). On `N`, exit cleanly with no changes to disk.
9. **Install the zombie.** Run
   `zombiectl zombie install --from .usezombie/platform-ops/ --json`.
   Capture `zombie_id` and `webhook_url` from the response. If the
   response lacks `webhook_url`, surface the captured JSON and exit
   with "install JSON missing webhook_url — file an issue".
10. **Self-verify the webhook before the user pastes it into GitHub.**
    Compute HMAC-SHA256 of a synthetic payload using the secret from
    step 5, then curl the receiver:

    ```bash
    PAYLOAD='{"zen":"install-test"}'
    SIG=$(printf %s "$PAYLOAD" | openssl dgst -sha256 -hmac "$WEBHOOK_SECRET" | awk '{print $2}')
    curl -fsS -X POST "https://api.usezombie.com/v1/webhooks/${ZOMBIE_ID}" \
      -H "X-Hub-Signature-256: sha256=${SIG}" \
      -H "Content-Type: application/json" \
      -d "$PAYLOAD"
    ```

    Expect HTTP `202`. On non-202, network failure, or HMAC mismatch,
    print the response verbatim and stop — **do not** advance to the
    GitHub-paste step. A broken webhook the user discovers hours later
    when production CD actually fails destroys the wedge demo.
11. **Print the post-install summary** including the webhook URL
    (`https://api.usezombie.com/v1/webhooks/{zombie_id}`), the
    one-time secret (only on the first-install path from step 5), and
    GitHub-config instructions: payload URL, content type
    `application/json`, secret, events `Workflow runs`. Tell the user
    exactly where to paste it (`Settings → Webhooks → Add webhook` on
    the repo).
12. **Smoke test.** Run
    `zombiectl steer {zombie_id} "morning health check"` and stream
    the response inline. If the round-trip exceeds 60 seconds, print
    "zombie installed but first response slow — check
    `zombiectl events {zombie_id}`" and stop. Otherwise the user has a
    working zombie posting to their Slack within ~60 seconds of
    invoking this skill.

## Common Mistakes

| # | Mistake | Fix |
|---|---|---|
| 1 | Skipping `zombiectl doctor` and going straight to install | Doctor is the only sanctioned readiness check. Always run it first. |
| 2 | Passing the JSON body via `--data '<JSON>'` instead of `--data @-` | Secret bytes leak into shell history and `ps` output. Always pipe JSON on stdin. |
| 3 | Re-running the skill on a second repo and overwriting `github.webhook_secret` | The credential `add` default skip-if-exists prevents this. Don't pass `--force` unless rotating. The skill prompts reuse-vs-scope on second install (step 5). |
| 4 | Asking the user to paste the webhook into GitHub before verifying it works | Always self-verify with `openssl dgst -sha256 -hmac` + `curl` to the receiver first (step 10). |
| 5 | Hard-coding Claude Code's `AskUserQuestion` in the body prose | Use the host's question primitive when present, or inline natural-language prompts otherwise. This skill must work in Amp, Codex CLI, and OpenCode too. |
| 6 | Calling the model-caps endpoint directly | Doctor's `tenant_provider` block already carries resolved values. Never add a network dependency for what doctor already has. |
| 7 | Asking the user about LLM model or BYOK | Out-of-band — see [`references/byok-handoff.md`](references/byok-handoff.md). The skill never holds an LLM api_key. |

## When to Load References

- **Credential resolution failed** → [`references/credential-resolution.md`](references/credential-resolution.md)
- **Skill exited with an error** → [`references/failure-modes.md`](references/failure-modes.md)
- **User wants to switch to BYOK** → [`references/byok-handoff.md`](references/byok-handoff.md)

## Out of Scope

- Non-GitHub-Actions CI providers (GitLab, CircleCI, Jenkins) — future milestone.
- BYOK setup — out-of-band, via `zombiectl credential add <name> --data @-`
  + `zombiectl tenant provider add --credential <name>`. The skill never
  asks about, holds, or stores an LLM api_key.
- GitHub App for auto-webhook configuration — separate milestone (next
  install-UX iteration). Until then, manual paste is the documented
  compromise; the in-flow self-test (step 10) is the safety net.
