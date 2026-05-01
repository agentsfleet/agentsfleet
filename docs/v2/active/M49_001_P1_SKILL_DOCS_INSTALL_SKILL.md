# M49_001: Install-Skill — `/usezombie-install-platform-ops`

**Prototype:** v2.0.0
**Milestone:** M49
**Workstream:** 001
**Date:** May 01, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — packaging-blocking. The wedge launch demo IS this skill installing platform-ops on Customer Zero's repo. Without it the runtime ships but no one knows how to put a zombie on their repo without running raw `zombiectl install --from`.
**Categories:** SKILL, DOCS
**Batch:** B3 — depends on substrate (M40-M46) being shippable end-to-end.
**Branch:** feat/m48-m51-onboarding-spec-fixes
**Depends on:**
- **M40** (worker substrate) — install must take effect immediately on the running worker.
- **M42** (steer CLI) — for the post-install smoke-test event.
- **M43** (webhook ingest) — the install-skill is the user-facing surface for webhook setup; it generates the HMAC secret, stores it in the workspace `github` credential, and prints the webhook URL inline.
- **M44** (install contract + doctor) — skill calls `zombiectl doctor --json` first; doctor's `tenant_provider` block is the source of the model + cap the skill pins into frontmatter.
- **M45** (vault structured creds) — skill calls `zombiectl credential set <name> --data='<json>'` for each tool credential.
- **M46** (frontmatter schema) — skill generates a single SKILL.md, not SKILL+TRIGGER.
- **M48** (BYOK provider + credit-pool billing) — locks doctor's `tenant_provider` block, the user-named credential model, the platform default (Fireworks Kimi K2.6 + 256K cap), and the deletion of the workspace-scoped `/credentials/llm` route. The install-skill is platform-managed-by-default; BYOK setup happens out-of-band via M48's `tenant provider set`.

**Canonical architecture:**
- [`docs/architecture/user_flow.md`](../../architecture/user_flow.md) §8.1-§8.5 (authoring, installing, triggering — this skill IS that workflow automated) and §8.7 (model + cap origin under both postures).
- [`docs/architecture/billing_and_byok.md`](../../architecture/billing_and_byok.md) §10 — the model-caps endpoint shape, rotation, and Cloudflare configuration.
- [`docs/architecture/scenarios/01_default_install.md`](../../architecture/scenarios/01_default_install.md) and [`02_byok.md`](../../architecture/scenarios/02_byok.md) — the two end-to-end scenarios this skill enables.

---

## Implementing agent — read these first

1. `~/.claude/skills/gstack/` — read 3-5 gstack skills to learn the canonical SKILL.md pattern (`name`, `description`, `when_to_use`, `tags`, body in markdown).
2. https://github.com/resend/resend-cli/tree/main#agent-skills — Resend's pattern for host-neutral CLI skills with `variables:` frontmatter.
3. `samples/platform-ops/SKILL.md` (post-M46) — the canonical SKILL.md the skill generates.
4. M44's `zombiectl doctor` interface — the skill calls `zombiectl doctor --json` first and reads the `tenant_provider` block.
5. M48's spec — for the doctor block shape, the user-named credential model, and the platform default values.
6. `usezombie/skills/` repo (NEW — this milestone creates it; see §1).
7. `usezombie/skills-evals/` repo (NEW — this milestone creates it; eval suite for skills).

---

## Overview

**Skill name (locked by /plan-ceo-review):** The user-facing invocation is **`/usezombie-install-platform-ops`** in every host — Claude Code, Amp, Codex CLI, OpenCode. One slash-command, one install procedure, one screenshot. Future skills follow the same dashed pattern: `/usezombie-steer`, `/usezombie-doctor`.

**Install procedure (same for every host):** Drop the SKILL.md directory into the host's skills folder. For Claude Code: `~/.claude/skills/usezombie-install-platform-ops/SKILL.md`. Other hosts use their equivalent path. No plugin manifest, no per-host packaging fork — one SKILL.md, one directory name, one slash-command.

The directory inside the `usezombie/skills` distribution repo (`usezombie/skills/usezombie-install-platform-ops/`) and the cache path (`~/.cache/usezombie/skills/usezombie-install-platform-ops/<tag>/`) match the slash-command name to keep everything aligned end-to-end.

**Goal (testable):** A user runs `/usezombie-install-platform-ops` in any supported host. The skill:

1. Calls `zombiectl doctor --json`. If any check fails (auth, workspace binding), surfaces the failure with the `auth login` hint and exits.
2. Reads doctor's `tenant_provider` block to learn the active model + context cap. Under default (platform-managed): `provider=fireworks`, `model=accounts/fireworks/models/kimi-k2.6`, `context_cap_tokens=256000`. Under BYOK (operator already configured via M48): real values from `tenant_providers`.
3. Detects the user's repo: reads `.github/workflows/*.yml`, `fly.toml`, `Dockerfile`, `pyproject.toml`, `package.json`. Infers deploy target. If no GH workflow, bails clearly.
4. Resolves three variables via host-neutral natural-language Q&A: `slack_channel`, `prod_branch_glob`, `cron_opt_in`. The skill never asks about LLM model or BYOK — those are doctor-driven and out-of-band respectively.
5. Generates the GitHub webhook secret locally (32 CSPRNG bytes, base64). Displays it once; never logs, never re-displays, never persists outside the vault.
6. Resolves four tool credentials in order `op` (1Password CLI) → env var → masked interactive prompt: `fly`, `slack`, `github` (carrying `{webhook_secret, api_token}`), optional `upstash`. Stores each via `zombiectl credential set <name> --data='<json>'` (M45 upsert surface).
7. Fetches the canonical platform-ops template from a pinned tag of the main repo. Caches at `~/.cache/usezombie/skills/usezombie-install-platform-ops/<tag>/`.
8. Generates `.usezombie/platform-ops/SKILL.md` in the user's repo. Substitutes the three variables plus the model + cap from doctor's block (resolved values under platform; sentinels `model: ""` / `context_cap_tokens: 0` under BYOK).
9. Calls `zombiectl install --from .usezombie/platform-ops/`. Captures `{zombie_id, webhook_url}` from JSON output.
10. Prints a post-install summary including the webhook URL (`https://api.usezombie.com/v1/webhooks/{zombie_id}`), the one-time secret, and GitHub-config instructions.
11. Calls `zombiectl steer {id} "morning health check"` and streams the response inline.

The user has a working zombie installed in <60 seconds from skill invocation, posting to their Slack.

**Problem:** Without this skill, an external user has to read `samples/platform-ops/README.md`, manually run 4-6 `zombiectl credential set` commands, edit a SKILL.md to substitute their values, run install, run steer. The friction kills onboarding. The wedge needs a one-command install.

**Solution summary:** Two repos created — `usezombie/skills` (the install-skills, drop-in to `~/.claude/skills/` or fetched via `https://usezombie.sh/skills.md`) and `usezombie/skills-evals` (eval suite). The install skill is a single SKILL.md conforming to the gstack-conformant + `x-usezombie:` extension format (M46). Variables are declared in frontmatter, resolved by the host. Templates are fetched from the main repo at a pinned tag, cached locally. The skill is the install UX — there is no separate dashboard "Webhook setup" card or "First credential" wizard to maintain; this skill replaces both.

---

## Locked design points

These are the contracts the skill must hit. Every decision below is final — there's no "current main / target contract" framing because all upstream specs (M43, M44, M45, M46, M48) ship together for the v2.0 launch.

### Webhook URL is the flat path

`https://api.usezombie.com/v1/webhooks/{zombie_id}` — no workspace prefix, no source suffix. That's what the receiver in M43 listens on. The skill prints this verbatim in the post-install summary.

### Credentials use the user-named, opaque-JSON model (M45)

The skill calls `zombiectl credential set <name> --data='<json>'` (upsert; same surface used for the BYOK credential under M48). For each tool:

| Credential | Vault name | JSON body shape | Notes |
|---|---|---|---|
| GitHub | `github` (workspace-scoped, M43 convention) | `{"webhook_secret": "<S>", "api_token": "<PAT>"}` | Skill generates `webhook_secret` locally (see below); `api_token` is a GH PAT |
| Fly | `fly` (workspace-scoped) | `{"api_token": "<value>", "host": "<value>"}` | Optional `host` if the operator has a non-default Fly endpoint |
| Slack | `slack` (workspace-scoped) | `{"api_token": "<value>"}` | Single field |
| Upstash | `upstash` (workspace-scoped, optional) | `{"redis_url": "<value>", "redis_token": "<value>"}` | Skipped if not detected |

The vault credential name is a **convention**, not a per-zombie pointer. The webhook ingest resolver (M43) looks the credential up by `name = trigger.source` automatically, so the skill does not write a `signature.secret_ref:` field into frontmatter.

### Skill generates the HMAC webhook secret locally

32 random bytes from the host's CSPRNG, base64-encoded. The skill:

1. Generates the secret in-process before any vault write.
2. Stores it via `zombiectl credential set github --data='{"webhook_secret":"<generated>","api_token":"<resolved>"}'`.
3. Displays it once during the install flow as part of the post-install summary, instructing the user to paste it into GitHub's webhook settings UI.
4. Never logs it, never persists it outside the vault, never re-displays it. Subsequent rotation: the user runs `zombiectl credential set github --data='{"webhook_secret":"<new>","api_token":"<existing>"}'` themselves.

The resolver assumes the secret is already in vault by the time webhook traffic arrives — this contract lives in the skill, not in M43.

### Skill prints the webhook URL inline (`zombiectl install` JSON mode)

`zombiectl install` returns `webhook_url` in JSON mode but does NOT print it in pretty mode. The skill consumes JSON mode and prints the user-actionable URL + secret + GitHub-config instructions in its post-install summary. This is what the dashboard would otherwise need a "Webhook setup" card for; punting that card by handling it here.

If a future spec adds pretty-mode printing of `webhook_url`, the skill's behaviour remains correct (it would still parse JSON mode and print its own formatted block).

### Frontmatter shape (no `signature.secret_ref`)

```yaml
x-usezombie:
  trigger:
    source: github                  # default credential lookup: name="github", field="webhook_secret"
    # credential_name: github-prod  # optional override, only when one workspace has multiple GH integrations
```

No `secret_ref:` line. The resolver's convention-by-name lookup makes the pointer redundant.

### Three variables — and only three

The skill collects exactly: `slack_channel`, `prod_branch_glob`, `cron_opt_in`. There is no `byok_provider_credential` variable. **BYOK setup is out-of-band**, before or after install, via M48's contract:

```bash
zombiectl credential set <user-chosen-name> --data '{"provider":"...","api_key":"...","model":"..."}'
zombiectl tenant provider set --credential <user-chosen-name>
```

The install-skill never asks about BYOK, never writes to `tenant_providers`, never holds an LLM api_key. It reads `zombiectl doctor --json`'s `tenant_provider` block and branches on `mode` to decide what to write into frontmatter.

### Doctor consumes the model + cap (skill does not call the model-caps endpoint)

`zombiectl doctor --json` returns a `tenant_provider` block carrying the resolved model + cap. Synth-default for tenants with no row; real values for tenants who ran `tenant provider set`. The skill writes:

| Mode | Frontmatter `model` | Frontmatter `context_cap_tokens` |
|---|---|---|
| `platform` | resolved (e.g. `accounts/fireworks/models/kimi-k2.6`, taken verbatim from doctor's block) | resolved (e.g. `256000`) |
| `byok` | `""` (sentinel) | `0` (sentinel) |

Under BYOK the worker overlays the sentinel values from `core.tenant_providers` at trigger time (M48 contract). The visible sentinels (`""` / `0`) make it obvious to a human reading the file that "this zombie inherits from tenant config." Hand-edits that strip the keys still work — absent-key is the safety net.

The model-caps endpoint at `https://api.usezombie.com/_um/da5b6b3810543fe108d816ee972e4ff8/model-caps.json` is consumed by the platform-side resolver (for the synth-default constants and the per-model token-rate cache) and by `zombiectl tenant provider set` (M48). **The install-skill never calls this endpoint directly.** This keeps the skill simple — read doctor, branch on mode, write frontmatter.

### Workspace-scoped `/credentials/llm` is gone

M48 removes the `PUT|GET|DELETE /v1/workspaces/{workspace_id}/credentials/llm` route entirely. Any historical M49 prose referencing that route was stale; the skill never used it. Listed here so the implementer doesn't reintroduce it.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| New repo `usezombie/skills/` | NEW REPO | Public OSS skills registry |
| `usezombie/skills/usezombie-install-platform-ops/SKILL.md` | NEW | The install skill itself. Directory name matches the slash-command. |
| `usezombie/skills/README.md` | NEW | Single install procedure for every host: drop the directory into the host's skills folder. Same `/usezombie-install-platform-ops` invocation everywhere. |
| `usezombie/skills/SKILLS.md` | NEW | Top-level index listing available skills with one-line descriptions. |
| New repo `usezombie/skills-evals/` | NEW REPO | Eval suite for skills (cross-cuts M51 docs) |
| `usezombie/skills-evals/usezombie-install-platform-ops/` | NEW | Fixture repos + eval harness |
| `samples/platform-ops/SKILL.md` (this repo) | EDIT | Verify it parses cleanly under M46's schema; this is the template the skill fetches |
| `samples/fixtures/m49-install-skill-fixtures/` | NEW | Test fixture repos: `gh-actions-fly/`, `gh-actions-only/`, `no-ci/` (each with the right files for repo-detection paths) |

> **Note:** The skill's SKILL.md lives in a SEPARATE repo (`usezombie/skills`), not in `usezombie/usezombie`. This separation lets the skill move on its own release cadence and lets `usezombie.sh` CDN serve it as a stable URL.

---

## Sections (implementation slices)

### §1 — `usezombie/skills` repo bootstrap

Create the repo with: `README.md`, `LICENSE` (Apache-2.0 or MIT, match main repo), `usezombie-install-platform-ops/SKILL.md`, a top-level `SKILLS.md` index. The repo is pure markdown — no build artifacts.

Public URL: `https://github.com/usezombie/skills`. Mirrored at `https://usezombie.sh/skills.md` (serves the index) and `https://usezombie.sh/skills/usezombie-install-platform-ops/SKILL.md` (serves the skill body) once M51's CDN is up.

> **Implementation default:** repo structure mirrors gstack's convention: one directory per skill, each with a SKILL.md.

### §2 — `usezombie/skills-evals` repo bootstrap

Eval suite: for each skill, a fixture repo + an expected-output assertion. Run on every PR. For `usezombie-install-platform-ops`, the eval:

1. Spins up a fixture repo (`gh-actions-fly/` from `samples/fixtures/m49-install-skill-fixtures/`).
2. Runs the skill against a mocked `zombiectl` (faked `doctor` pass with a synth-default `tenant_provider` block, faked `install` returns success with a JSON `webhook_url`, faked `steer` returns canned response).
3. Asserts: skill detected `gh-actions-fly` → asked for `slack_channel` → resolved creds via env → generated `.usezombie/platform-ops/SKILL.md` with substitutions → invoked install with the right `--from` path.
4. LLM-judge eval: was the skill's user-facing prose clear? Did it explain the failure modes? Score >= 7/10 over 5 trial runs.

> **Implementation default:** eval harness is a Node script using existing test runner. No new framework. Trials run in CI nightly; threshold gates skill releases.

### §3 — The install skill body

`usezombie/skills/usezombie-install-platform-ops/SKILL.md`:

```yaml
---
name: usezombie-install-platform-ops
description: Install a usezombie platform-ops zombie on this repo — watches GH Actions CD failures, posts diagnoses to Slack.
when_to_use: When the user asks to "install platform-ops", "set up the deploy zombie", or "watch my CI for failures".
tags: [usezombie, platform-ops, install, devops, sre]
author: usezombie
version: 0.1.0

x-usezombie:
  variables:
    - name: slack_channel
      prompt: "Which Slack channel should the zombie post to?"
      example: "#platform-ops"
      required: true
    - name: prod_branch_glob
      prompt: "What branch glob counts as 'production'?"
      default: "main"
    - name: cron_opt_in
      prompt: "Should the zombie also run a periodic health check (every 30 min)?"
      type: bool
      default: false
  template_url: "https://raw.githubusercontent.com/usezombie/usezombie/{tag}/samples/platform-ops/SKILL.md"
  template_pinned_tag: "v0.34.0"   # bumped on each skill release
---

# Body of SKILL.md

You are an installer for usezombie's platform-ops zombie. Your job is to set up
a working platform-ops zombie on the user's current repository.

## Plan

1. Run `zombiectl doctor --json`. If any check fails (auth_token_present: false,
   workspace_bound: false, etc.), surface the failure clearly and tell the user
   to run `zombiectl auth login` (or whatever the failed check requires).
   Then stop. Do not proceed to install on a broken environment.

2. Read doctor's `tenant_provider` block from the same JSON response. It carries
   `{mode, provider, model, context_cap_tokens, credential_ref?}`. Remember
   `mode` — you'll branch on it at step 7. Never call the model-caps endpoint
   directly; doctor already has the resolved values.

3. Detect the user's repo:
   - Look for `.github/workflows/*.yml`. If absent, tell the user this skill
     supports GitHub Actions only in v1; non-GH CI is in a future version. Stop.
   - Look for `fly.toml` (Fly.io), `Dockerfile` (Docker-based), `pyproject.toml`
     or `package.json` (the language). Use this to set sensible defaults in
     the SKILL.md prose ("your zombie reasons over fly logs and your CI runs").
   - Note the repo name from `git remote get-url origin` for use in messaging.

4. Ask the user the three variables declared in frontmatter, ONE AT A TIME. Use
   the host's native question primitive (Claude Code's AskUserQuestion, Amp's
   prompt, Codex's stdin, OpenCode's form). If the host doesn't have a question
   primitive, fall back to inline natural-language prompts ("Which Slack
   channel? Reply with the channel name."). Re-prompt up to 2x on empty input;
   exit on the third empty answer with a hint.

5. Generate the GitHub webhook secret locally: 32 bytes from the host's CSPRNG,
   base64-encoded. Hold it in process memory only — never log, never write to
   disk outside the vault.

6. Resolve the four tool credentials. For each of `github`, `fly`, `slack`,
   `upstash` (optional, skip if not detected), assemble the JSON body and run
   `zombiectl credential set <name> --data='<json>'`. The resolution order for
   each field value is: `op read 'op://<vault>/<item>/<field>'` → env var
   `ZOMBIE_CRED_<NAME>_<FIELD>` → masked interactive prompt.

   - **`github`** — JSON body `{"webhook_secret": "<step-5-value>", "api_token":
     "<resolved>"}`. The `webhook_secret` field is the secret you just generated;
     `api_token` is the GitHub PAT (resolved through op/env/prompt).
   - **`fly`** — JSON body `{"api_token": "<resolved>", "host": "<resolved or
     omit>"}`. `host` is optional.
   - **`slack`** — JSON body `{"api_token": "<resolved>"}`.
   - **`upstash`** (skip if no `*.upstash.io` references in the repo) — JSON
     body `{"redis_url": "<resolved>", "redis_token": "<resolved>"}`.

   Surface stderr verbatim if `zombiectl credential set` fails (API down, auth
   expired, etc.) and exit cleanly.

7. Fetch the canonical platform-ops template from the URL in `template_url`,
   substituting `{tag}` with `template_pinned_tag`. Cache at
   `~/.cache/usezombie/skills/usezombie-install-platform-ops/<tag>/SKILL.md`.
   On cache hit, use it; on miss + offline, fail with a clear message.

8. Generate `.usezombie/platform-ops/SKILL.md` in the user's repo. Substitute:
   - `{{slack_channel}}` → user's value
   - `{{prod_branch_glob}}` → user's value
   - `{{cron_opt_in}}` → user's choice (if true, add the cron block to the
     `x-usezombie.trigger.cron:` section; if false, omit)
   - `{{repo}}` → repo name from git remote
   - `{{model}}` and `{{context_cap_tokens}}` based on doctor's `tenant_provider.mode`:
     - `mode=platform` → write doctor's resolved values verbatim
       (e.g. `model: accounts/fireworks/models/kimi-k2.6`,
       `context_cap_tokens: 256000`).
     - `mode=byok` → write sentinels (`model: ""`, `context_cap_tokens: 0`).
       The worker overlays from `tenant_providers` at trigger time.

9. If the directory `.usezombie/platform-ops/` already exists in the user's
   repo, refuse to overwrite without `--force`. Ask: "Existing
   `.usezombie/platform-ops/` found. Overwrite? (y/N)". On no, stop.

10. Run `zombiectl install --from .usezombie/platform-ops/` in JSON mode.
    Capture `{zombie_id, webhook_url}` from the response.

11. Run `zombiectl steer {id} "morning health check"` and stream the response
    inline. This proves credentials, network, executor, and Slack are all
    wired correctly before any production webhook fires.

12. Print the post-install summary:

    ```
    Platform-ops zombie installed (id: {zombie_id}).

    Add this webhook to your GH repo (Settings → Webhooks → Add webhook):
      URL:    https://api.usezombie.com/v1/webhooks/{zombie_id}
      Secret: <one-time displayed value — copy now, won't be shown again>
      Events: workflow_run

    The secret is already stored in this workspace's vault as the `github`
    credential, field `webhook_secret`. The webhook receiver verifies HMAC
    against this stored value automatically.

    To steer manually any time:
      zombiectl steer {zombie_id} "<message>"

    To remove:
      zombiectl kill {zombie_id}
    ```

## Failure modes

If any step fails, print the exact error and stop. Do not silently retry. Do not
half-install. Examples:
- `zombiectl doctor` says auth missing → "Run `zombiectl auth login` first."
- Repo lacks `.github/workflows/` → "GitHub Actions only in v1."
- `op` errors with auth → "Run `op signin` first."
- Fetch fails + cache miss → "Cannot fetch template. Try again with internet."
- `zombiectl credential set` fails → surface stderr, exit.
- Steer round-trip times out (>60s) → "Zombie installed but first response slow
  — check `zombiectl events {id}`."

## Out of scope

- Non-GitHub CI providers (GitLab, CircleCI, Jenkins) — future version.
- BYOK setup — out-of-band, via `zombiectl credential set <name>` +
  `zombiectl tenant provider set --credential <name>` (M48). The skill never
  asks about, holds, or stores an LLM api_key.
- Bash one-liner installer — use this skill instead; it's portable across all
  agent CLIs that read SKILL.md.
```

### §4 — Repo detection logic

The skill's body has natural-language detection logic. The agent (Claude Code etc.) reads `.github/workflows/*.yml`, `fly.toml`, etc. via its file-read tool. The skill doesn't need a separate "detector binary" — the LLM is the detector, given the patterns to look for.

> **Implementation default:** if multiple workflow files exist, ask the user which one is the production deploy workflow. Default to the file with `deploy` in its name; if multiple, prompt.

### §5 — Credential resolution order

The skill's body specifies the resolution order (op → env → prompt). The agent runs the commands. No new code in the runtime — this is pure SKILL.md prose driving existing CLI commands.

### §6 — Template fetch + cache

Fetch URL from frontmatter `template_url`, substitute `{tag}`. The agent uses its HTTP/fetch tool. Cache to `~/.cache/usezombie/skills/usezombie-install-platform-ops/<tag>/`. Cache key is the tag — bumping the tag invalidates older caches automatically.

### §7 — usezombie.sh CDN serving

Out of scope here (covered by M51). The skill is fetchable from a raw GitHub URL on day 1; usezombie.sh CDN is a follow-up convenience.

---

## Frontmatter shape — what the skill writes

### Platform-managed install (default — Scenario 01)

```yaml
x-usezombie:
  model: accounts/fireworks/models/kimi-k2.6   # from doctor's tenant_provider.model
  context:
    context_cap_tokens: 256000                 # from doctor's tenant_provider.context_cap_tokens
    tool_window: auto
    memory_checkpoint_every: 5
    stage_chunk_threshold: 0.75
```

### BYOK install (Scenario 02 — operator already ran `tenant provider set`)

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

---

## Interfaces

```
Skill invocation (same name in every host):
  Claude Code / Amp / Codex CLI / OpenCode:   /usezombie-install-platform-ops

Skill input (variables, resolved per host):
  slack_channel: string (required)
  prod_branch_glob: string (default "main")
  cron_opt_in: boolean (default false)

Skill output (filesystem state after success):
  .usezombie/platform-ops/SKILL.md created in user's CWD with substituted
    variables and the model + cap pinned from doctor's tenant_provider block
    (resolved values under platform; sentinels under BYOK).
  Zombie installed in user's tenant + workspace via zombiectl install.
  Vault populated with the four tool credentials (github, fly, slack, optional
    upstash) via zombiectl credential set.
  zombiectl steer round-trip printed to stdout.
  Post-install summary printed: webhook URL + one-time secret + GH-config
    instructions + steer/kill examples.

Eval contract (usezombie/skills-evals):
  Per fixture repo: assert skill produced expected substituted SKILL.md.
  Per skill: LLM-judge prose clarity score >= 7/10 over 5 trials.
```

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| `zombiectl doctor` reports `auth_token_present: false` | User not logged in | Skill prints the doctor JSON, hints `zombiectl auth login`, exits |
| Repo lacks `.github/workflows/` | Non-GH-CI repo | Skill stops with "GH Actions only in v1" message |
| `.usezombie/platform-ops/` exists | Re-running on same repo | Skill prompts overwrite (y/N); on N, exits cleanly |
| Template fetch fails + no cache | Offline | Skill prints clear "no template, try online" message |
| `zombiectl credential set` fails | API down or auth expired | Skill captures stderr, surfaces verbatim, exits |
| Steer round-trip times out (>60s) | Worker not picking up event | Skill prints "zombie installed but first response slow — check `zombiectl events {id}`" |
| Variable resolution: empty value | User typo | Skill re-prompts up to 2x; on 3rd empty, exits with hint |
| `zombiectl install` returns no `webhook_url` in JSON | API contract regression | Skill prints captured JSON for debugging, exits with "install JSON missing webhook_url — file an issue" |

---

## Invariants

1. **Skill never runs `zombiectl install` until `zombiectl doctor` passes.** Hard precondition.
2. **Skill never overwrites `.usezombie/platform-ops/` without explicit consent.** Refuse without `--force`.
3. **Skill is host-neutral.** Variables resolved via the host's primitive, OR fall back to inline natural-language prompts. NEVER hard-codes Claude Code's AskUserQuestion.
4. **Templates are version-pinned.** Skill never fetches `main` — always a pinned tag. Tag bumps are explicit skill releases.
5. **Skill never holds an LLM api_key.** BYOK setup is out-of-band via M48's `tenant provider set`. The skill reads doctor's `tenant_provider` block to learn model + cap; the api_key never crosses the skill boundary.
6. **Skill never calls the model-caps endpoint directly.** Doctor's response is the source of model + cap.
7. **The HMAC webhook secret is generated in-process, displayed once, stored in vault, never re-displayed.**

---

## Test Specification

| Test | Asserts |
|------|---------|
| `test_doctor_fail_aborts_install` | Mock doctor failure → skill stops before any other step; user sees clear message |
| `test_repo_detection_gh_actions_fly` | Fixture repo with `.github/workflows/deploy.yml + fly.toml` → skill identifies fly deploy target |
| `test_repo_detection_no_ci_aborts` | Fixture repo without `.github/workflows/` → skill stops with clear "GH only in v1" |
| `test_credential_resolution_op_first` | `op` installed and returns value → skill uses op output, doesn't prompt |
| `test_credential_resolution_env_fallback` | No `op`, env var set → skill uses env var |
| `test_credential_resolution_interactive_fallback` | No `op`, no env → skill prompts (and masks) |
| `test_template_fetch_pinned_tag` | Skill fetches from `<pinned-tag>` URL → cache populated |
| `test_template_cache_hit_offline` | Run skill twice → second run uses cache, succeeds offline |
| `test_overwrite_refuses_without_force` | `.usezombie/platform-ops/` exists → skill prompts; on N, no changes to disk |
| `test_doctor_platform_mode_writes_resolved_frontmatter` | Doctor reports `mode=platform`, `model=…kimi-k2.6`, `context_cap_tokens=256000` → generated frontmatter has resolved values |
| `test_doctor_byok_mode_writes_sentinels` | Doctor reports `mode=byok` → generated frontmatter has `model: ""` and `context_cap_tokens: 0` |
| `test_webhook_secret_displayed_once_stored_in_vault` | Skill generates secret, runs `credential set github`, prints secret in summary; secret bytes never appear in any log file produced by the skill |
| `test_e2e_install_to_first_steer` | All happy path → steer response printed to stdout |
| `test_eval_llm_judge_clarity` | LLM judge over 5 trial runs → average score >= 7/10 (eval-suite test, runs nightly) |

Fixtures in `samples/fixtures/m49-install-skill-fixtures/`:
- `gh-actions-fly/` — full happy path
- `gh-actions-only/` — no fly, just GH
- `no-ci/` — should abort

---

## Acceptance Criteria

- [ ] `usezombie/skills` repo created and public
- [ ] `usezombie/skills-evals` repo created with at least 1 eval (this skill)
- [ ] `usezombie-install-platform-ops/SKILL.md` parses cleanly under M46's schema
- [ ] All 14 tests pass (13 functional + 1 eval-suite)
- [ ] Manual: Customer Zero (author) runs `/usezombie-install-platform-ops` on the usezombie repo itself, ends with a working zombie posting to author's Slack
- [ ] Manual: same author runs `/usezombie-install-platform-ops` after running `tenant provider set --credential <name>` first → generated frontmatter carries the BYOK sentinels (`model: ""`, `context_cap_tokens: 0`)
- [ ] Manual: same author runs `/usezombie-install-platform-ops` in at least one non-Claude host (Amp, Codex CLI, or OpenCode), produces byte-identical `.usezombie/platform-ops/SKILL.md`
- [ ] Skill is fetchable from `https://raw.githubusercontent.com/usezombie/skills/main/usezombie-install-platform-ops/SKILL.md` AND (post-M51) `https://usezombie.sh/skills/usezombie-install-platform-ops/SKILL.md`
- [ ] Single SKILL.md directory with no host-specific packaging fork — same drop-in works for every supported host

---

## Out of Scope

- Non-GitHub-Actions CI providers (M{N+}_001)
- BYOK setup inside the install-skill (out-of-band via M48)
- Bash one-liner installer (rejected — cross-platform shell-test cost too high)
- Skills for other zombie shapes — separate milestones if/when those shapes ship
- Skill auto-update mechanism (user re-runs skill to get newer template)
- Skill telemetry (covered by M51 install-pingback)
- Direct calls to the model-caps endpoint from the skill (doctor handles this)
- Dashboard "Webhook setup" card (the skill's post-install summary replaces it)
