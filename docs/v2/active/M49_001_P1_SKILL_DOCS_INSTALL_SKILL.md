# M49_001: Install-Skill — `/usezombie-install-platform-ops`

**Prototype:** v2.0.0
**Milestone:** M49
**Workstream:** 001
**Date:** May 03, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — packaging-blocking. The wedge launch demo IS this skill installing platform-ops on Customer Zero's repo. Without it the runtime ships but no one knows how to put a zombie on their repo without running raw `zombiectl zombie install --from`.
**Categories:** SKILL, DOCS
**Batch:** B3 — depends on substrate (M40-M46) being shippable end-to-end.
**Branch:** feat/m49-install-skill
**Depends on:**
- **M40** (worker substrate) — install must take effect immediately on the running worker.
- **M42** (steer CLI) — for the post-install smoke-test event.
- **M43** (webhook ingest) — the install-skill is the user-facing surface for webhook setup; it generates the HMAC secret, stores it in the workspace `github` credential, and prints the webhook URL inline. M43's only dependency is the receiver itself (`POST /v1/webhooks/{zombie_id}` with HMAC verification). The skill verifies the webhook in-flow by computing HMAC-SHA256 of a synthetic payload and curling the receiver — no `zombiectl webhook test` subcommand needed.
- **M44** (install contract + doctor) — skill calls `zombiectl doctor --json` first; doctor's `tenant_provider` block is the source of the model + cap the skill pins into frontmatter.
- **M45** (vault structured creds) — skill calls `zombiectl credential add <name> --data @-` for each tool credential, piping JSON on stdin so secret bytes never appear in shell history or process argv. Default upsert is skip-if-exists; `--force` overwrites. The skill relies on this default to avoid clobbering a workspace's shared `github.webhook_secret` on a second install.
- **M46** (frontmatter schema) — skill generates `SKILL.md` plus `TRIGGER.md`; `SKILL.md` carries behavior prose, `TRIGGER.md` carries the typed trigger/tool/budget/context envelope.
- **M48** (BYOK provider + credit-pool billing) — locks doctor's `tenant_provider` block, the user-named credential model, the platform default (Fireworks Kimi K2.6 + 256K cap), and the deletion of the workspace-scoped `/credentials/llm` route. The install-skill is platform-managed-by-default; BYOK setup happens out-of-band via M48's `tenant provider add`.

**Canonical architecture:**
- [`docs/architecture/user_flow.md`](../../architecture/user_flow.md) §8.1-§8.5 (authoring, installing, triggering — this skill IS that workflow automated) and §8.7 (model + cap origin under both postures).
- [`docs/architecture/billing_and_byok.md`](../../architecture/billing_and_byok.md) §10 — the model-caps endpoint shape, rotation, and Cloudflare configuration.
- [`docs/architecture/scenarios/01_default_install.md`](../../architecture/scenarios/01_default_install.md) and [`02_byok.md`](../../architecture/scenarios/02_byok.md) — the two end-to-end scenarios this skill enables.

**Distribution reference:** [resend/resend-cli's agent skills](https://github.com/resend/resend-cli/tree/main#agent-skills) — Resend's pattern of shipping a host-neutral SKILL.md with a CLI, distributed via `npx skills add <org>/<repo>`. M49 mirrors that exact shape.

---

## Implementing agent — read these first

1. [resend/resend-cli's SKILL.md frontmatter pattern](https://github.com/resend/resend-cli) — `name`, `description`, `license`, `metadata` (author/version/source/requires), `inputs`, `references`. The `usezombie-install-platform-ops/SKILL.md` mirrors this structure.
2. `samples/platform-ops/` (already exists in this repo) — the canonical zombie template the skill substitutes variables into and installs via `zombiectl zombie install --from <path>`.
3. M44's `zombiectl doctor` interface — the skill calls `zombiectl doctor --json` first and reads the `tenant_provider` block.
4. M48's spec — for the doctor block shape, the user-named credential model, and the platform default values.
5. M45's `zombiectl credential add --data @-` interface, default skip-if-exists semantics, and `--force` flag.
6. M43's webhook receiver contract (`POST /v1/webhooks/{zombie_id}`, HMAC-SHA256 verification against vault `github.webhook_secret`). The skill computes its own HMAC and curls the receiver in-flow — no separate test subcommand.

---

## Overview

**Skill name (locked):** The user-facing invocation is **`/usezombie-install-platform-ops`** in every host — Claude Code, Amp, Codex CLI, OpenCode. One slash-command, one install procedure, one screenshot. Future skills follow the same dashed pattern: `/usezombie-steer`, `/usezombie-doctor`.

**Two artifacts, two homes — keep these terms straight:**

| Artifact | Lives at (this repo) | Distributed via | Installed to user's machine at |
|----------|----------------------|------------------|--------------------------------|
| **Zombie template** — `SKILL.md` + `TRIGGER.md` + `README.md` defining platform-ops behavior | `samples/platform-ops/` (already exists) | Bundled inside `@usezombie/zombiectl` npm package | `~/.config/usezombie/samples/platform-ops/` (npm postinstall copy) |
| **Agent skill** — host slash-command that drives the install flow | `skills/usezombie-install-platform-ops/SKILL.md` (NEW) | `npx skills add usezombie/usezombie` (Resend pattern) | Symlinked into `~/.claude/skills/`, `~/.codex/skills/`, `~/.amp/skills/`, `~/.opencode/skills/` (whichever exist) |

The directory name `usezombie-install-platform-ops/` matches the slash-command in every place — repo, package, host symlink target.

**Goal (testable):** A user runs the two-command bootstrap once:

```bash
npm install -g @usezombie/zombiectl    # CLI binary + bundled samples → ~/.config/usezombie/samples/
npx skills add usezombie/usezombie     # symlinks /usezombie-* skills into host skill paths
```

Then in any supported host, invokes `/usezombie-install-platform-ops`. The skill:

1. Calls `zombiectl doctor --json`. If any check fails (auth, workspace binding), surfaces the failure with the `auth login` hint and exits.
2. Reads doctor's `tenant_provider` block to learn the active model + context cap. Under default (platform-managed): `provider=fireworks`, `model=accounts/fireworks/models/kimi-k2.6`, `context_cap_tokens=256000`. Under BYOK (operator already configured via M48): real values from `tenant_providers`.
3. Detects the user's repo: reads `.github/workflows/*.yml`, `fly.toml`, `Dockerfile`, `pyproject.toml`, `package.json`. Infers deploy target. If no GitHub workflow, bails clearly.
4. Resolves three variables via host-neutral natural-language Q&A: `slack_channel`, `prod_branch_glob`, `cron_schedule`. The skill never asks about LLM model or BYOK — those are doctor-driven and out-of-band respectively.
5. Generates the GitHub webhook secret locally (32 CSPRNG bytes, base64) **only on first install for the workspace**. On second install with an existing `github.webhook_secret`, prompts the user to either reuse the workspace-shared secret or scope a per-zombie credential (see `Locked design points → webhook secret`).
6. Resolves four tool credentials in order `op` (1Password CLI) → env var → masked interactive prompt: `fly`, `slack`, `github` (carrying `{webhook_secret, api_token}`), optional `upstash`. Stores each via `zombiectl credential add <name> --data @-` (M45 upsert surface, default skip-if-exists), with JSON piped on stdin.
7. Reads the canonical platform-ops template from `~/.config/usezombie/samples/platform-ops/` (laid down by zombiectl's npm postinstall). No URL fetch, no cache. Missing dir = npm install corrupted; print one-line repair hint and exit.
8. Generates `.usezombie/platform-ops/SKILL.md` and `.usezombie/platform-ops/TRIGGER.md` in the user's repo. Substitutes the three variables plus the model + cap from doctor's block (resolved values under platform; sentinels `model: ""` / `context_cap_tokens: 0` under BYOK).
9. Calls `zombiectl zombie install --from .usezombie/platform-ops/`. Captures `{zombie_id, webhook_url}` from JSON output.
10. Verifies the webhook works before asking the user to paste it into GitHub: computes HMAC-SHA256 of a synthetic payload (e.g. `{"zen":"install-test"}`) using the secret it just stored, then curls `POST https://api.usezombie.com/v1/webhooks/{zombie_id}` with the `X-Hub-Signature-256: sha256=<computed>` header. Asserts 202. On failure (non-202, network error, HMAC mismatch), surfaces the response and stops — does not advance to the GitHub-paste step.
11. Prints a post-install summary including the webhook URL (`https://api.usezombie.com/v1/webhooks/{zombie_id}`), the one-time secret (only on the new-secret path), and GitHub-config instructions.
12. Calls `zombiectl steer {id} "morning health check"` and streams the response inline.

The user has a working zombie installed and webhook-verified in <60 seconds from skill invocation, posting to their Slack.

**Problem:** Without this skill, an external user has to read `samples/platform-ops/README.md`, manually run 4-6 `zombiectl credential add` commands, edit a SKILL.md to substitute their values, run install, run steer, then verify the webhook by hand. The friction kills onboarding. The wedge needs a one-command install (after the two-command bootstrap).

**Solution summary:** Two new directories in this repo — `skills/usezombie-install-platform-ops/` (the agent skill, distributed via `npx skills add usezombie/usezombie`) and `tests/skill-evals/usezombie-install-platform-ops/` (eval suite). The zombie template stays at `samples/platform-ops/` (already exists; M49 edits it for substitution placeholders + morning-health-check prose). The npm package bundles `samples/` and `skills/`; postinstall copies samples to `~/.config/usezombie/samples/`. The skill reads templates from there, substitutes variables, drives `zombiectl zombie install --from`, verifies the webhook, and runs the smoke-test steer. The skill IS the install UX — there is no separate dashboard "Webhook setup" card or "First credential" wizard to maintain; this skill replaces both.

---

## Prerequisites (user-side)

Document these in the skill body's Installation section so any AI agent loading the SKILL.md walks the user through them on a fresh machine.

```bash
npm install -g @usezombie/zombiectl     # CLI + bundled zombie templates + samples postinstall
npx skills add usezombie/usezombie      # symlinks /usezombie-* skills into host skill paths
```

If `npx skills` is unavailable for the user's environment, the documented fallback is a manual symlink:

```bash
ln -s "$(npm root -g)/@usezombie/zombiectl/skills/usezombie-install-platform-ops" \
  ~/.claude/skills/usezombie-install-platform-ops
```

(Same shape for `~/.codex/skills/`, `~/.amp/skills/`, `~/.opencode/skills/`.)

Both paths live in the skill's Installation section so the agent can dictate them on cold-start machines.

---

## Locked design points

These are the contracts the skill must hit. Every decision below is final — all upstream specs (M43, M44, M45, M46, M48) ship together for the v2.0 launch.

### Webhook URL is the flat path

`https://api.usezombie.com/v1/webhooks/{zombie_id}` — no workspace prefix, no source suffix. That's what the receiver in M43 listens on. The skill prints this verbatim in the post-install summary.

### Credentials use the user-named, opaque-JSON model (M45)

The skill calls `zombiectl credential add <name> --data @-` (M45's upsert; default skip-if-exists, `--force` to overwrite). JSON is piped on stdin. The skill must not pass secret JSON through command arguments, because that leaks through shell history and process inspection. For each tool:

| Credential | Vault name | JSON body shape | Notes |
|---|---|---|---|
| GitHub | `github` (workspace-scoped, M43 convention) | `{"webhook_secret": "<S>", "api_token": "<PAT>"}` | Skill generates `webhook_secret` only on first install for the workspace; on second install, prompts reuse-or-scope (see below); `api_token` always resolved per-install via op → env → prompt |
| Fly | `fly` (workspace-scoped) | `{"api_token": "<value>", "host": "<value>"}` | Optional `host` if the operator has a non-default Fly endpoint |
| Slack | `slack` (workspace-scoped) | `{"api_token": "<value>"}` | Single field |
| Upstash | `upstash` (workspace-scoped, optional) | `{"redis_url": "<value>", "redis_token": "<value>"}` | Skipped if not detected |

The vault credential name is a **convention**, not a per-zombie pointer. The webhook ingest resolver (M43) looks the credential up by `name = trigger.source` automatically, so the skill does not write a `signature.secret_ref:` field into frontmatter.

### Skill generates the HMAC webhook secret locally — but only when one doesn't exist

32 random bytes from the host's CSPRNG, base64-encoded. The skill:

1. Checks if vault credential `github` already has a `webhook_secret` field (read via `zombiectl credential get github --json`, which returns field presence without echoing secret bytes — this is M45's contract).
2. **First install for the workspace** (no existing secret): generates the secret in-process, stores it via `zombiectl credential add github --data @-`, displays it once during the post-install summary.
3. **Second install for the workspace** (secret exists): prompts the user with two options:
   - **A) Reuse the workspace-shared secret** (recommended) — one secret protects every GitHub-triggered zombie in the workspace; one rotation rotates all. Skill writes nothing new to the `github` credential's `webhook_secret` field; api_token still upserted. Post-install summary instructs the user to use the same secret already configured for prior zombies.
   - **B) Scope a per-zombie credential** — generates a new secret, stores under credential name `github-{zombie_slug}`, writes `credential_name: github-{zombie_slug}` into the generated TRIGGER.md (the M43 override). Two credentials, two rotation surfaces, smaller blast radius per zombie.
4. Never logs the secret, never persists it outside the vault, never re-displays it after the post-install summary. Subsequent rotation: user runs `zombiectl credential add github --force --data @-` and pipes the replacement JSON on stdin.

The resolver assumes the secret is already in vault by the time webhook traffic arrives — this contract lives in the skill, not in M43.

### Skill verifies webhook in-flow via curl + HMAC (Goal step 10)

After `zombiectl zombie install` returns success, the skill self-verifies the webhook before asking the user to paste it into GitHub. It computes HMAC-SHA256 of a synthetic payload using the secret it just stored, then curls the receiver with the signed payload. Reference shape:

```bash
PAYLOAD='{"zen":"install-test"}'
SIG=$(printf %s "$PAYLOAD" | openssl dgst -sha256 -hmac "$WEBHOOK_SECRET" | awk '{print $2}')
curl -fsS -X POST "https://api.usezombie.com/v1/webhooks/${ZOMBIE_ID}" \
  -H "X-Hub-Signature-256: sha256=${SIG}" \
  -H "Content-Type: application/json" \
  -d "$PAYLOAD"
```

The skill expects `202`. On non-202, network error, or HMAC mismatch, it surfaces the response verbatim and stops *before* the GitHub-paste step. No `zombiectl webhook test` subcommand is required — the skill owns the verification by composing M43's existing receiver contract with `openssl` and `curl` (both ubiquitous on Mac/Linux).

This is the in-flow check that prevents shipping the user a broken webhook they discover hours later when production CD actually fails.

### Skill prints the webhook URL inline (`zombiectl install` JSON mode)

`zombiectl zombie install` returns `webhook_url` in JSON mode but does NOT print it in pretty mode. The skill consumes JSON mode and prints the user-actionable URL + secret + GitHub-config instructions in its post-install summary. This is what the dashboard would otherwise need a "Webhook setup" card for; punting that card by handling it here.

### TRIGGER.md shape (no `signature.secret_ref`)

```yaml
x-usezombie:
  trigger:
    source: github                              # default credential lookup: name="github", field="webhook_secret"
    # credential_name: github-zmb_01HX9N3K…    # populated only when user picks "scope per-zombie credential" at install
```

No `secret_ref:` line. The resolver's convention-by-name lookup makes the pointer redundant.

### Three variables — and only three

The skill collects exactly: `slack_channel`, `prod_branch_glob`, `cron_schedule`. There is no `byok_provider_credential` variable. **BYOK setup is out-of-band**, before or after install, via M48's contract:

```bash
op read 'op://<vault>/<item>/api_key' |
  jq -Rn '{provider:"fireworks", api_key: input, model:"accounts/fireworks/models/kimi-k2.6"}' |
  zombiectl credential add <user-chosen-name> --data @-
zombiectl tenant provider add --credential <user-chosen-name>
```

The install-skill never asks about BYOK, never writes to `tenant_providers`, never holds an LLM api_key. It reads `zombiectl doctor --json`'s `tenant_provider` block and branches on `mode` to decide what to write into frontmatter.

`cron_schedule` is a string (e.g. `*/30 * * * *`), default empty. Empty = no cron block in the generated TRIGGER.md. Earlier drafts used a `cron_opt_in` boolean tied to a hardcoded `*/30` schedule; the string is more honest and the cost is zero.

### Vault layout is user-dependent

The skill body documents the resolution order — `op read 'op://<your-vault>/<your-item>/<field>'` → env var `ZOMBIE_CRED_<NAME>_<FIELD>` → masked interactive prompt — but does NOT prescribe a vault name or item-naming convention. Each user has their own `op` layout. The skill prompts for the field if `op` doesn't return one, then proceeds.

### Doctor consumes the model + cap (skill does not call the model-caps endpoint)

`zombiectl doctor --json` returns a `tenant_provider` block carrying the resolved model + cap. Synth-default for tenants with no row; real values for tenants who ran `tenant provider add`. The skill writes:

| Mode | Frontmatter `model` | Frontmatter `context_cap_tokens` |
|---|---|---|
| `platform` | resolved (e.g. `accounts/fireworks/models/kimi-k2.6`, taken verbatim from doctor's block) | resolved (e.g. `256000`) |
| `byok` | `""` (sentinel) | `0` (sentinel) |

Under BYOK the worker overlays the sentinel values from `core.tenant_providers` at trigger time (M48 contract). The visible sentinels (`""` / `0`) make it obvious to a human reading the file that "this zombie inherits from tenant config." Hand-edits that strip the keys still work — absent-key is the safety net.

The model-caps endpoint at `https://api.usezombie.com/_um/da5b6b3810543fe108d816ee972e4ff8/model-caps.json` is consumed by the platform-side resolver (for the synth-default constants and the per-model token-rate cache) and by `zombiectl tenant provider add` (M48). **The install-skill never calls this endpoint directly.** This keeps the skill simple — read doctor, branch on mode, write frontmatter.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `skills/usezombie-install-platform-ops/SKILL.md` | NEW | The agent skill. Resend-pattern frontmatter + body sections. Distributed via `npx skills add usezombie/usezombie`. |
| `skills/usezombie-install-platform-ops/references/credential-resolution.md` | NEW | Reference doc for credential resolution order, op layouts, env var naming. Pointed at by frontmatter `references:`. |
| `skills/usezombie-install-platform-ops/references/failure-modes.md` | NEW | Reference doc enumerating every failure mode + the recovery hint the skill prints. |
| `skills/usezombie-install-platform-ops/references/byok-handoff.md` | NEW | Reference doc explaining the BYOK out-of-band flow (M48 contract) for users who want to switch posture. |
| `skills/README.md` | NEW | Top-level explainer: what the skills folder is, the `npx skills add usezombie/usezombie` install command, the symlink fallback. |
| `tests/skill-evals/usezombie-install-platform-ops/` | NEW | Fixture repos + eval harness. Basic file-generation eval (mock zombiectl, assert substituted SKILL.md + TRIGGER.md match expected). |
| `tests/skill-evals/fixtures/` | NEW | Test fixture repos: `gh-actions-fly/`, `gh-actions-only/`, `no-ci/`. |
| `package.json` (npm package manifest for `@usezombie/zombiectl`) | EDIT | Add `samples/` and `skills/` to `files:` array. Add `postinstall` script entry. |
| `scripts/postinstall.js` | NEW | Defensive postinstall: copies `samples/` to `~/.config/usezombie/samples/`. Idempotent. Skips on errors (never crashes `npm install`). |
| `samples/platform-ops/SKILL.md` | EDIT | Add the morning-health-check prose if not already present (eval requires it). Verify it parses cleanly under M46's schema. |
| `samples/platform-ops/TRIGGER.md` | EDIT | Add the variable substitution placeholders (`{{slack_channel}}`, `{{prod_branch_glob}}`, `{{cron_schedule}}`, `{{model}}`, `{{context_cap_tokens}}`). |

> **Note:** No new GitHub repos are created. Both the agent skill and its evals live in this repo. Distribution is via the npm package (samples bundle) + `npx skills add` (skill symlink).

---

## Sections (implementation slices)

### §1 — `skills/` folder bootstrap

Create `skills/` at the repo root (mirrors Resend's layout). Top-level `skills/README.md` documents the install commands. One subdirectory per skill. M49 adds only `usezombie-install-platform-ops/`; future skills follow the same shape.

The npm package's `files:` array includes `skills/` so the directory ships inside `@usezombie/zombiectl`. After `npx skills add usezombie/usezombie` runs, every supported host (Claude Code, Codex CLI, Amp, OpenCode) has a symlink at `<host-skills-dir>/usezombie-install-platform-ops/` pointing into the npm install location.

### §2 — `tests/skill-evals/` bootstrap

For each skill, a fixture repo + an expected-output assertion. Run on every PR. For `usezombie-install-platform-ops`, the eval:

1. Spins up a fixture repo (`gh-actions-fly/` from `tests/skill-evals/fixtures/`).
2. Runs the skill against a mocked `zombiectl` (faked `doctor` pass with a synth-default `tenant_provider` block, faked `install` returns success with a JSON `webhook_url`, faked `webhook test` returns 200, faked `steer` returns canned response).
3. Asserts: skill detected `gh-actions-fly` → asked for `slack_channel` → resolved creds via env → generated `.usezombie/platform-ops/SKILL.md` with substitutions → invoked install with the right `--from` path → ran webhook test before printing GitHub-paste instructions.
4. LLM-judge eval: was the skill's user-facing prose clear? Did it explain the failure modes? Score >= 7/10 over 5 trial runs.

Eval harness: existing test runner (`make test` or equivalent). No new framework. Trials run in CI nightly; threshold gates skill releases.

### §3 — The install skill body (Resend-pattern)

`skills/usezombie-install-platform-ops/SKILL.md`:

```yaml
---
name: usezombie-install-platform-ops
description: >
  Install a usezombie platform-ops zombie on this repo — watches GitHub Actions
  CD failures and posts evidenced diagnoses to Slack. Always load this skill
  before running `zombiectl zombie install` for platform-ops; it knows the
  doctor preflight, credential resolution order, webhook setup, and smoke-test
  steps that prevent silent failures.
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

Before running this skill, ensure `zombiectl` is installed and the `/usezombie-*`
skills are symlinked into your host's skills directory.

```bash
zombiectl --version
```

If not found:

```bash
npm install -g @usezombie/zombiectl
```

If `/usezombie-install-platform-ops` is not available as a slash-command:

```bash
npx skills add usezombie/usezombie
```

Manual symlink fallback (if `npx skills` is unavailable):

```bash
ln -s "$(npm root -g)/@usezombie/zombiectl/skills/usezombie-install-platform-ops" \
  ~/.claude/skills/usezombie-install-platform-ops
```

(Same shape for `~/.codex/skills/`, `~/.amp/skills/`, `~/.opencode/skills/`.)

## Agent Protocol

This skill drives `zombiectl` non-interactively. Every `zombiectl` invocation
uses `--json` where the flag is supported, parses the JSON response, and
surfaces stderr verbatim on failure. Exit `0` = success, `1` = error.
Never proceed past a failed step. Never silently retry.

## Authentication

`zombiectl` auth is checked once via `zombiectl doctor --json`. If
`auth_token_present: false`, the skill prints `Run zombiectl auth login first`
and stops. The skill never logs in on the user's behalf.

Tool credentials (`fly`, `slack`, `github`, `upstash`) resolve in this order
per field:

1. `op read 'op://<your-vault>/<your-item>/<field>'` — uses your existing 1Password layout. The skill does not prescribe a vault or item naming.
2. Env var `ZOMBIE_CRED_<NAME>_<FIELD>` (e.g. `ZOMBIE_CRED_FLY_API_TOKEN`).
3. Masked interactive prompt.

JSON bodies are piped through stdin into `zombiectl credential add <name> --data @-` so secret bytes never appear in shell history or process argv.

## Plan

(steps 1-12 from the spec's Goal section, fleshed out into prose the agent reads top-to-bottom)

## Common Mistakes

| # | Mistake | Fix |
|---|---------|-----|
| 1 | Skipping `zombiectl doctor` and going straight to install | Doctor is the only sanctioned readiness check. Always run it first. |
| 2 | Passing the JSON body via `--data '<JSON>'` instead of `--data @-` | Secret bytes leak into shell history and `ps` output. Always pipe JSON on stdin. |
| 3 | Re-running the skill on a second repo and overwriting `github.webhook_secret` | M45's default skip-if-exists prevents this. Don't pass `--force` unless rotating. The skill prompts reuse-vs-per-zombie-scope on second install. |
| 4 | Asking the user to paste the webhook into GitHub before verifying it works | Always self-verify with `openssl dgst -sha256 -hmac` + `curl` to the receiver first. A broken webhook the user discovers hours later destroys the wedge demo. |
| 5 | Hardcoding Claude Code's AskUserQuestion in the body prose | Use the host's question primitive OR fall back to inline natural-language prompts. Skill must work in Amp, Codex CLI, OpenCode too. |
| 6 | Calling the model-caps endpoint directly | Doctor's `tenant_provider` block already carries resolved values. Never add a network dependency for what doctor already has. |

## When to Load References

- **Credential resolution failed** → [references/credential-resolution.md](references/credential-resolution.md)
- **Skill exited with an error** → [references/failure-modes.md](references/failure-modes.md)
- **User wants to switch to BYOK** → [references/byok-handoff.md](references/byok-handoff.md)

## Out of Scope

- Non-GitHub CI providers (GitLab, CircleCI, Jenkins) — future milestone.
- BYOK setup — out-of-band, via `zombiectl credential add <name> --data @-` + `zombiectl tenant provider add --credential <name>` (M48). The skill never asks about, holds, or stores an LLM api_key.
- GitHub App for auto-webhook configuration — separate milestone (next install-UX iteration). Until then, manual paste is the documented compromise.
```

### §4 — Repo detection logic

The skill's body has natural-language detection logic. The agent (Claude Code etc.) reads `.github/workflows/*.yml`, `fly.toml`, etc. via its file-read tool. The skill doesn't need a separate "detector binary" — the LLM is the detector, given the patterns to look for.

**Defaults:** if multiple workflow files exist, ask the user which one is the production deploy workflow. Default to the file with `deploy` in its name; if multiple, prompt with the list. If `git remote get-url origin` returns multiple remotes, use `origin`; if `origin` is absent, ask.

### §5 — Credential resolution order

The skill body specifies the resolution order (op → env → prompt). The agent runs the commands. No new code in the runtime — pure SKILL.md prose driving existing CLI commands.

### §6 — Template read (no fetch, no cache)

The skill reads `~/.config/usezombie/samples/platform-ops/SKILL.md` and `~/.config/usezombie/samples/platform-ops/TRIGGER.md` directly. The npm postinstall (this milestone, §7) is the layer that puts them there.

If the directory is missing (npm install corrupted, postinstall skipped, manual cleanup), the skill prints:

```
Cannot find platform-ops template at ~/.config/usezombie/samples/platform-ops/.
Reinstall: npm install -g @usezombie/zombiectl
```

…and exits.

No URL fetch. No cache. The npm package version is the template version — bumping `@usezombie/zombiectl` is the upgrade path.

### §7 — npm postinstall script

`scripts/postinstall.js` (Node, no external deps):

1. Read source dir: `<npm-install-prefix>/samples/`.
2. Target dir: `~/.config/usezombie/samples/`.
3. If target exists and contents match source (cheap manifest hash), no-op.
4. If target exists with different contents, back up to `~/.config/usezombie/samples.backup-<ts>/` then copy fresh.
5. If target missing, mkdir + copy.
6. On any error (permission denied, FS full, weird platform): print one-line warning, exit 0. **Never crash `npm install`.**

The skill's failure mode for missing samples (§6) covers the postinstall-skipped case.

---

## TRIGGER.md shape — what the skill writes

### Platform-managed install (default — Scenario 01)

`TRIGGER.md`:

```yaml
x-usezombie:
  model: accounts/fireworks/models/kimi-k2.6   # from doctor's tenant_provider.model
  context:
    context_cap_tokens: 256000                 # from doctor's tenant_provider.context_cap_tokens
    tool_window: auto
    memory_checkpoint_every: 5
    stage_chunk_threshold: 0.75
```

### BYOK install (Scenario 02 — operator already ran `tenant provider add`)

`TRIGGER.md`:

```yaml
x-usezombie:
  model: ""                                    # sentinel: worker overlays from tenant_providers
  context:
    context_cap_tokens: 0                      # sentinel: worker overlays from tenant_providers
    tool_window: auto
    memory_checkpoint_every: 5
    stage_chunk_threshold: 0.75
```

The worker treats `model == ""` (or absent) and `context_cap_tokens == 0` (or absent) as "resolve at trigger time from `tenant_providers`." The two sentinels are independent — either can be populated in frontmatter and overlaid by tenant config or vice versa.

### Per-zombie webhook secret variant (only when user picks reuse-vs-scope option B)

```yaml
x-usezombie:
  trigger:
    source: github
    credential_name: github-zmb_01HX9N3K…      # explicit M43 override
```

---

## Interfaces

```
Skill invocation (same name in every host):
  Claude Code / Amp / Codex CLI / OpenCode:   /usezombie-install-platform-ops

Skill input (variables, resolved per host):
  slack_channel:    string (required)
  prod_branch_glob: string (default "main")
  cron_schedule:    string (default "" — empty omits cron block)

Skill output (filesystem state after success):
  .usezombie/platform-ops/SKILL.md and TRIGGER.md created in user's CWD with
    substituted variables and the model + cap pinned from doctor's
    tenant_provider block (resolved values under platform; sentinels under
    BYOK; per-zombie credential_name override only when user picked option B).
  Zombie installed in user's tenant + workspace via `zombiectl zombie install`.
  Vault populated with the four tool credentials (github, fly, slack, optional
    upstash) via `zombiectl credential add --data @-`. On second install,
    `github.webhook_secret` reused or new `github-{slug}` credential created.
  M43 webhook test passed before GitHub-paste instructions printed.
  zombiectl steer round-trip printed to stdout.
  Post-install summary printed: webhook URL + (one-time secret on first install) +
    GitHub-config instructions + steer/kill examples.

Eval contract (tests/skill-evals/):
  Per fixture repo: assert skill produced expected substituted SKILL.md and TRIGGER.md.
  Per skill: LLM-judge prose clarity score >= 7/10 over 5 trials.
```

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| `zombiectl doctor` reports `auth_token_present: false` | User not logged in | Skill prints the doctor JSON, hints `zombiectl auth login`, exits |
| Repo lacks `.github/workflows/` | Non-GitHub-CI repo | Skill stops with "GitHub Actions only in v1" message |
| `.usezombie/platform-ops/` exists | Re-running on same repo | Skill prompts overwrite (y/N); on N, exits cleanly |
| `~/.config/usezombie/samples/platform-ops/` missing | npm postinstall skipped or install corrupted | Skill prints reinstall hint, exits |
| `zombiectl credential add` fails | API down or auth expired | Skill captures stderr, surfaces verbatim, exits |
| Existing `github.webhook_secret` in vault | Workspace already has GitHub-triggered zombies | Skill prompts reuse-vs-scope-per-zombie; user picks |
| Webhook test fails after install | HMAC mismatch, receiver bug, network | Skill prints the test response, points at `zombiectl events {id}`, exits — does not advance to GitHub-paste step |
| Steer round-trip times out (>60s) | Worker not picking up event | Skill prints "zombie installed but first response slow — check `zombiectl events {id}`" |
| Variable resolution: empty value | User typo | Skill re-prompts up to 2x; on 3rd empty, exits with hint |
| `zombiectl zombie install` returns no `webhook_url` in JSON | API contract regression | Skill prints captured JSON for debugging, exits with "install JSON missing webhook_url — file an issue" |
| npm postinstall fails (permission, FS full) | User's machine quirks | Postinstall logs warning, exits 0; npm install succeeds; skill prints reinstall hint when it can't find samples |

---

## Invariants

1. **Skill never runs `zombiectl zombie install` until `zombiectl doctor` passes.** Hard precondition.
2. **Skill never overwrites `.usezombie/platform-ops/` without explicit consent.** Refuse without `--force`.
3. **Skill is host-neutral.** Variables resolved via the host's primitive, OR fall back to inline natural-language prompts. NEVER hard-codes Claude Code's AskUserQuestion.
4. **Skill never holds an LLM api_key.** BYOK setup is out-of-band via M48's `tenant provider add`. The skill reads doctor's `tenant_provider` block to learn model + cap; the api_key never crosses the skill boundary.
5. **Skill never calls the model-caps endpoint directly.** Doctor's response is the source of model + cap.
6. **The HMAC webhook secret is generated only on first-install for a workspace; reused or per-zombie-scoped on subsequent installs.** Never re-displayed after the post-install summary in which it was generated.
7. **Skill verifies the webhook works before asking the user to paste it into GitHub.** No silent broken-webhook ship.
8. **Skill reads templates from local FS (`~/.config/usezombie/samples/`), never from a network URL.** The npm package version is the version pin.
9. **npm postinstall never crashes `npm install`.** FS errors log a warning and exit 0; the skill's missing-samples failure mode catches the consequence at invocation time.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_doctor_fail_aborts_install` | Mock doctor failure → skill stops before any other step; user sees clear message |
| `test_repo_detection_gh_actions_fly` | Fixture repo with `.github/workflows/deploy.yml + fly.toml` → skill identifies fly deploy target |
| `test_repo_detection_no_ci_aborts` | Fixture repo without `.github/workflows/` → skill stops with clear "GitHub Actions only in v1" |
| `test_credential_resolution_op_first` | `op` installed and returns value → skill uses op output, doesn't prompt |
| `test_credential_resolution_env_fallback` | No `op`, env var set → skill uses env var |
| `test_credential_resolution_interactive_fallback` | No `op`, no env → skill prompts (and masks) |
| `test_template_read_from_config_dir` | Skill reads `~/.config/usezombie/samples/platform-ops/` → no network call attempted |
| `test_template_missing_aborts_with_repair_hint` | `~/.config/usezombie/samples/platform-ops/` absent → skill prints `npm install -g @usezombie/zombiectl` hint and exits |
| `test_overwrite_refuses_without_force` | `.usezombie/platform-ops/` exists → skill prompts; on N, no changes to disk |
| `test_doctor_platform_mode_writes_resolved_frontmatter` | Doctor reports `mode=platform`, `model=…kimi-k2.6`, `context_cap_tokens=256000` → generated frontmatter has resolved values |
| `test_doctor_byok_mode_writes_sentinels` | Doctor reports `mode=byok` → generated frontmatter has `model: ""` and `context_cap_tokens: 0` |
| `test_webhook_secret_generated_on_first_install` | No existing `github.webhook_secret` → skill generates, stores, displays once |
| `test_webhook_secret_reuse_prompted_on_second_install` | Existing `github.webhook_secret` → skill prompts reuse-vs-scope; on reuse, no new secret generated; on scope, `github-{slug}` credential created and TRIGGER.md gets `credential_name:` override |
| `test_webhook_secret_never_in_argv_or_logs` | Skill generates secret, runs `credential add github --data @-`, prints secret in summary; secret bytes never appear in command argv, generated files, or any log file produced by the skill |
| `test_webhook_curl_runs_before_github_paste_step` | Mock receiver → skill computes HMAC + curls receiver with synthetic payload after install, before printing GitHub paste instructions |
| `test_webhook_curl_failure_aborts_before_github_paste` | Mock receiver returns 401 (HMAC mismatch) or 5xx → skill prints response, does not print GitHub paste instructions |
| `test_webhook_curl_uses_openssl_and_curl_only` | Skill's verification path uses `openssl` + `curl` (no zombiectl subcommand) — verifies the in-flow check has zero new CLI surface dependency |
| `test_e2e_install_to_first_steer` | All happy path → steer response printed to stdout |
| `test_npm_postinstall_copies_samples` | `npm install -g @usezombie/zombiectl` in fresh env → `~/.config/usezombie/samples/platform-ops/` exists with template files |
| `test_npm_postinstall_idempotent` | Two consecutive `npm install` runs → second is no-op (no error, no overwrite of unchanged files) |
| `test_npm_postinstall_failure_does_not_crash_install` | Permission-denied target dir → postinstall logs warning, npm install exits 0 |
| `test_eval_llm_judge_clarity` | LLM judge over 5 trial runs → average score >= 7/10 (eval-suite test, runs nightly) |

Fixtures in `tests/skill-evals/fixtures/`:
- `gh-actions-fly/` — full happy path
- `gh-actions-only/` — no fly, just GitHub
- `no-ci/` — should abort

---

## Acceptance Criteria

- [ ] `skills/usezombie-install-platform-ops/SKILL.md` lives in this repo with Resend-pattern frontmatter (name, description, license, metadata, inputs, references)
- [ ] `skills/usezombie-install-platform-ops/references/{credential-resolution,failure-modes,byok-handoff}.md` all present and referenced from frontmatter
- [ ] `tests/skill-evals/usezombie-install-platform-ops/` exists with the basic file-generation eval + LLM-judge prose clarity eval
- [ ] `package.json` includes `samples/` and `skills/` in `files:`; `postinstall` script wired
- [ ] `scripts/postinstall.js` is defensive: idempotent, never crashes `npm install` on FS errors
- [ ] `samples/platform-ops/SKILL.md` body teaches the agent to handle "morning health check" by fetching GH Actions runs on `prod_branch_glob`, Fly app status, optional Upstash ping, posting Slack summary
- [ ] Generated `.usezombie/platform-ops/SKILL.md` and `TRIGGER.md` parse cleanly under M46's schema
- [ ] All 22 tests pass (21 functional + 1 eval-suite)
- [ ] Manual: `npm install -g @usezombie/zombiectl` on a clean Mac → `~/.config/usezombie/samples/platform-ops/` populated
- [ ] Manual: `npx skills add usezombie/usezombie` → `~/.claude/skills/usezombie-install-platform-ops/` symlink present (and equivalents for other hosts where dirs exist)
- [ ] Manual: Customer Zero (author) runs `/usezombie-install-platform-ops` on the usezombie repo itself, ends with a working zombie posting to author's Slack
- [ ] Manual: same author runs `/usezombie-install-platform-ops` after running `tenant provider add --credential <name>` first → generated frontmatter carries the BYOK sentinels (`model: ""`, `context_cap_tokens: 0`)
- [ ] Manual: same author runs `/usezombie-install-platform-ops` a second time on a different repo in the same workspace → skill prompts reuse-vs-scope-per-zombie; both paths produce working zombies
- [ ] Manual: same author runs `/usezombie-install-platform-ops` in at least two non-Claude hosts (Amp + Codex CLI or OpenCode), produces byte-identical `.usezombie/platform-ops/SKILL.md` and `TRIGGER.md`
- [ ] No new GitHub repos created — agent skill, evals, samples all in this repo

---

## Out of Scope

- Non-GitHub-Actions CI providers (separate milestone)
- BYOK setup inside the install-skill (out-of-band via M48)
- Bash one-liner installer (rejected — `npm install -g` + `npx skills add` is the install path)
- Skills for other zombie shapes — separate milestones if/when those shapes ship
- Skill auto-update mechanism beyond `npm install -g @usezombie/zombiectl@latest` and `npx skills update usezombie/usezombie`
- Skill telemetry (covered by M51 install-pingback)
- Direct calls to the model-caps endpoint from the skill (doctor handles this)
- Dashboard "Webhook setup" card (the skill's post-install summary replaces it)
- **GitHub App for auto-webhook configuration** — biggest UX win available (eliminates manual paste, lost-secret recovery, broken-webhook-discovery delay), but its own substantial workstream (~5 days: app registration, OAuth, per-workspace install management, scope/permissions UX). Defer to a dedicated next-install-UX milestone. Until it ships, manual paste + in-flow webhook test is the documented compromise.
- Two-repo distribution (rejected — npm package + `npx skills add` is simpler and matches Resend's pattern)
- Integration eval tier against real `zombiectl` (rejected — manual Customer Zero acceptance + nightly LLM-judge eval is sufficient for v2 launch; revisit if regressions appear)
- Zombiectl `skills` subcommand (rejected — `zombiectl zombie install --from <path>` is the install verb; skill discovery is via `npx skills add`, not a parallel CLI namespace)
