# M49_001: Install-Skill — `/usezombie-install-platform-ops`

**Prototype:** v2.0.0
**Milestone:** M49
**Workstream:** 001
**Date:** Apr 25, 2026
**Status:** PENDING
**Priority:** P1 — packaging-blocking. The wedge launch demo IS this skill installing platform-ops on Customer Zero's repo. Without it the runtime ships but no one knows how to put a zombie on their repo without running raw `zombiectl install --from`.
**Categories:** SKILL, DOCS
**Batch:** B3 — depends on substrate (M40-M46) being shippable end-to-end.
**Branch:** feat/m49-install-skill (to be created)
**Depends on:** M40 (worker substrate — install must take effect immediately), M42 (steer CLI for the post-install demo), M43 (webhook ingest — if user opts into GH Actions trigger, install configures the webhook), M44 (install contract + doctor — skill calls `zombiectl doctor` before invoking install), M45 (vault structured creds — skill resolves credentials into structured form), M46 (frontmatter schema — skill generates a single SKILL.md, not SKILL+TRIGGER).

**Canonical architecture:** `docs/architecture/user_flow.md` §8.1-§8.5 (authoring, installing, triggering — this skill IS the §8.1-§8.2 workflow automated). Cap + model lookup design lives in `docs/architecture/billing_and_byok.md` §9 (see Discovery D1/D2 below).

---

## Cross-spec amendment (Apr 30, 2026 — folded from M43 review pass)

The M43 webhook-ingest review pinned several decisions that this skill must absorb because the install-skill is the operator-facing surface for webhook setup. These supersede the original §3 step 4 (credential resolution) and step 10 (post-install messaging) wherever they conflict.

**B1 — Webhook URL is `https://api.usezombie.com/v1/webhooks/{zombie_id}`.** Drop the `.../zombies/{id}/webhooks/github` form from the post-install message at the bottom of §3. Workspace prefix is wrong for webhooks, and the source-suffixed path is not the shipped `main` contract.

**B2 — Credentials are workspace-scoped opaque JSON, addressed by name** (M45 contract). The skill calls `zombiectl credential add <name> --data='<json>'`. For GitHub specifically: `<name> = "github"`, JSON body `{"webhook_secret": "<S>", "api_token": "<PAT>"}`. Both the credential name and its field names are conventions the skill follows so the SKILL.md frontmatter doesn't need a per-zombie `signature.secret_ref` pointer — the webhook ingest resolver looks the credential up by `name = trigger.source` automatically.

**B3 — Skill generates the HMAC webhook secret with high entropy.** 32 random bytes from the host's CSPRNG, base64-encoded. Skill displays it once during the install flow, instructs the operator to paste it into GitHub's webhook settings UI, and stores it via `zombiectl credential add github --data='{"webhook_secret":"<displayed>", ...}'`. The secret never logs, never persists outside vault, never re-displays. M43 assumes the secret already exists in vault by the time webhook traffic arrives — that contract lives here.

**B4 — Skill prints the webhook URL inline.** `zombiectl install` returns `webhook_url` in JSON mode but does NOT print it in pretty mode (verified at `zombiectl/src/commands/zombie.js:108-124`). The install-skill consumes JSON mode and prints the operator-actionable URL + secret + GitHub-config instructions in its post-install summary. This is what the dashboard would otherwise need a "Webhook setup" card for; punting that card by handling it here.

**B5 — Frontmatter shape: drop `signature.secret_ref`** when the trigger source is a known provider in `PROVIDER_REGISTRY`. The skill writes:

```yaml
x-usezombie:
  trigger:
    source: github                  # default credential lookup: name="github", field="webhook_secret"
    # credential_name: github-prod  # optional override, only when one workspace has multiple GH integrations
```

No `secret_ref:` line. The resolver at `src/cmd/serve_webhook_lookup.zig` migrates to convention-based lookup as part of M43.

**B6 — Variable resolution adjusted.** Drop `byok_provider_credential` from the variables list under §3 step 3. Current `main` stores BYOK credentials through the workspace-scoped `PUT /v1/workspaces/{workspace_id}/credentials/llm` route; the tenant-scoped `zombiectl provider set` posture remains the pending M48 target contract and is not part of the install-skill flow. Replace the slot with **nothing** (3 variables instead of 4): `slack_channel`, `prod_branch_glob`, `cron_opt_in`.

**B7 — `webhook_secret_ref` column is removed in M43.** No skill change required — the skill never wrote to it. Listed here only so the M49 implementer doesn't re-introduce the legacy pattern.

**B8 — Pretty-mode `webhook_url` print** in `zombiectl install` is technically a `zombiectl` concern, not a skill concern. The skill works around the pretty-mode gap by parsing JSON mode. If a future spec adds pretty-mode printing of `webhook_url`, the skill's behavior remains correct (it would still parse JSON mode and print its own formatted block).

---

## Implementing agent — read these first

1. `~/.claude/skills/gstack/` — read 3-5 gstack skills to learn the canonical SKILL.md pattern (`name`, `description`, `when_to_use`, `tags`, body in markdown).
2. https://github.com/resend/resend-cli/tree/main#agent-skills — Resend's pattern for host-neutral CLI skills with `variables:` frontmatter.
3. `samples/platform-ops/SKILL.md` (post-M46) — the canonical SKILL.md the skill generates.
4. M44's `zombiectl doctor` interface — the skill calls `zombiectl doctor --json` first.
5. `usezombie/skills/` repo (NEW — this milestone creates it; see §1).
6. `usezombie/skills-evals/` repo (NEW — this milestone creates it; eval suite for skills).

---

## Overview

**Skill name (locked by /plan-ceo-review on Apr 25, 2026):** The user-facing invocation is **`/usezombie-install-platform-ops`** in every host — Claude Code, Amp, Codex CLI, OpenCode. One slash-command, one install procedure, one screenshot. Future skills follow the same dashed pattern: `/usezombie-steer`, `/usezombie-doctor`.

**Install procedure (same for every host):** Drop the SKILL.md directory into the host's skills folder. For Claude Code: `~/.claude/skills/usezombie-install-platform-ops/SKILL.md`. Other hosts use their equivalent path. No plugin manifest, no per-host packaging fork — one SKILL.md, one directory name, one slash-command.

The directory inside the `usezombie/skills` distribution repo (`usezombie/skills/usezombie-install-platform-ops/`) and the cache path (`~/.cache/usezombie/skills/usezombie-install-platform-ops/<tag>/`) match the slash-command name to keep everything aligned end-to-end.

**Goal (testable):** A user runs `/usezombie-install-platform-ops` in any supported host. The skill:

1. Calls `zombiectl doctor --json`. If any check fails, surfaces the failure with the `auth login` hint and exits.
2. Detects the user's repo: reads `.github/workflows/*.yml`, `fly.toml`, `Dockerfile`, `pyproject.toml`, `package.json`. Infers deploy target.
3. Resolves variables (3 total — per B6) via host-neutral natural-language Q&A (NOT Claude-specific `AskUserQuestion`):
   - `slack_channel` (e.g., `#platform-ops`)
   - `prod_branch_glob` (e.g., `main` or `release/*`)
   - `cron_opt_in` (boolean, default false)
   - Current `main` keeps BYOK credential setup outside the install flow through the workspace-scoped `credentials/llm` route. The tenant-scoped `zombiectl provider set` posture remains the pending M48 target contract.
4. Resolves credentials in order: `op` (1Password CLI) → env vars → interactive prompt fallback. Stores via `zombiectl credential add` with structured fields.
5. Fetches the canonical platform-ops template from `https://raw.githubusercontent.com/usezombie/usezombie/<pinned-tag>/samples/platform-ops/SKILL.md`. Caches at `~/.cache/usezombie/skills/usezombie-install-platform-ops/<tag>/`.
6. Generates `.usezombie/platform-ops/SKILL.md` in the user's repo with substituted variables.
7. Calls `zombiectl install --from .usezombie/platform-ops/`.
8. Calls `zombiectl steer {id} "morning health check"` and prints the zombie's first response inline.

The user has a working zombie installed in <60 seconds from skill invocation, posting to their Slack.

**Problem:** Without this skill, an external operator has to read `samples/platform-ops/README.md`, manually run 4-6 `zombiectl credential add` commands, edit a SKILL.md to substitute their values, run install, run steer. The friction kills onboarding. The wedge needs a one-command install.

**Solution summary:** Two repos created — `usezombie/skills` (the install-skills, drop-in to `~/.claude/skills/` or fetched via `https://usezombie.sh/skills.md`) and `usezombie/skills-evals` (eval suite). The install skill is a single SKILL.md conforming to the gstack-conformant + `x-usezombie:` extension format (M46). Variables are declared in frontmatter, resolved by the host (Claude Code asks in dropdown, Codex CLI prompts on stdin, OpenCode renders form). Templates are fetched from the main repo at a pinned tag, cached locally. The skill is the install UX.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| New repo `usezombie/skills/` | NEW REPO | Public OSS skills registry |
| `usezombie/skills/usezombie-install-platform-ops/SKILL.md` | NEW | The install skill itself. Directory name matches the slash-command. |
| `usezombie/skills/README.md` | NEW | Single install procedure for every host: drop the directory into the host's skills folder. Same `/usezombie-install-platform-ops` invocation everywhere. |
| New repo `usezombie/skills-evals/` | NEW REPO | Eval suite for skills (cross-cuts M51 docs) |
| `usezombie/skills-evals/usezombie-install-platform-ops/` | NEW | Fixture repos + eval harness |
| `samples/platform-ops/SKILL.md` (this repo) | EDIT | Verify it parses cleanly under M46's schema; this is the template the skill fetches |
| `samples/fixtures/m49-install-skill-fixtures/` | NEW | Test fixture repos: `gh-actions-fly/`, `gh-actions-only/`, `no-ci/` (each with the right files for repo-detection paths) |

> **Note:** The skill's SKILL.md lives in a SEPARATE repo (`usezombie/skills`), not in `usezombie/usezombie`. This separation lets the skill move on its own release cadence and lets `usezombie.sh` CDN serve it as a stable URL.

---

## Sections (implementation slices)

### §1 — `usezombie/skills` repo bootstrap

Create the repo with: `README.md`, `LICENSE` (Apache-2.0 or MIT, match main repo), `usezombie-install-platform-ops/SKILL.md`, a top-level `SKILLS.md` index (lists all available skills with one-line descriptions). The repo is pure markdown — no build artifacts.

Public URL: `https://github.com/usezombie/skills`. Mirrored at `https://usezombie.sh/skills.md` (serves the index) and `https://usezombie.sh/skills/usezombie-install-platform-ops/SKILL.md` (serves the skill body) once M51's CDN is up.

> **Implementation default:** repo structure mirrors gstack's convention: one directory per skill, each with a SKILL.md.

### §2 — `usezombie/skills-evals` repo bootstrap

Eval suite: for each skill, a fixture repo + an expected-output assertion. Run on every PR. For `usezombie-install-platform-ops`, the eval:

1. Spins up a fixture repo (`gh-actions-fly/` from `samples/fixtures/m49-install-skill-fixtures/`).
2. Runs the skill against a mocked `zombiectl` (faked `doctor` pass, faked `install` returns success, faked `steer` returns canned response).
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
  # Current `main` keeps BYOK credential setup outside the install skill through
  # the workspace-scoped `credentials/llm` route. Tenant-scoped `provider set`
  # remains the pending M48 target contract.
  template_url: "https://raw.githubusercontent.com/usezombie/usezombie/{tag}/samples/platform-ops/SKILL.md"
  template_pinned_tag: "v0.34.0"   # bumped on each skill release
---

# Body of SKILL.md

You are an installer for usezombie's platform-ops zombie. Your job is to set up
a working platform-ops zombie on the user's current repository.

## Plan

1. Run `zombiectl doctor --json`. If any check fails, surface the failure clearly
   and tell the user to run `zombiectl auth login` (and any other failed check fix).
   Then stop. Do not proceed to install on a broken environment.

2. Detect the user's repo:
   - Look for `.github/workflows/*.yml`. If absent, tell the user this skill
     supports GitHub Actions only in v1; non-GH CI is in a future version. Stop.
   - Look for `fly.toml` (Fly.io), `Dockerfile` (Docker-based), `pyproject.toml`
     or `package.json` (the language). Use this to set sensible defaults in
     the SKILL.md prose ("your zombie reasons over fly logs and your CI runs").
   - Note the repo name from `git remote get-url origin` for use in messaging.

3. Ask the user the variables declared in frontmatter, ONE AT A TIME. Use the
   host's native question primitive (Claude Code's AskUserQuestion, Amp's prompt,
   Codex's stdin, OpenCode's form). If the host doesn't have a question
   primitive, fall back to inline natural-language prompts ("Which Slack channel?
   Reply with the channel name.").

4. Resolve credentials. M45 stores credentials as opaque JSON keyed by name, so the skill assembles a JSON body per provider and calls `zombiectl credential add <name> --data='<json>'` (per B2). For each of `fly`, `upstash` (optional), `slack`, `github`:
   - **`github`** is special (per B3). Generate the webhook secret locally: 32 bytes from the host's CSPRNG, base64-encoded. Display it once for the operator to paste into GitHub's webhook settings UI; never log it, never re-display it. Then resolve `api_token` (the GH PAT) via `op read` → env var `ZOMBIE_CRED_GITHUB_API_TOKEN` → masked interactive prompt. Final shape:
     `zombiectl credential add github --data='{"webhook_secret":"<generated>","api_token":"<resolved>"}'`
   - **`fly` / `upstash` / `slack`** resolve each structured field via the same op → env-var → masked-prompt fallback chain. The JSON body shape per provider is `{"api_token":"<value>"[, additional fields]}`; consult the provider's vault credential shape (see M45 conventions). Run:
     `zombiectl credential add <name> --data='<assembled-json>'`
   - The legacy single-flag form (`--api-token`, `--host`) is gone; M45 dropped typed credentials in favour of opaque JSON-data per name.

5. Fetch the canonical platform-ops template from the URL in `template_url`,
   substituting `{tag}` with `template_pinned_tag`. Cache at
   `~/.cache/usezombie/skills/usezombie-install-platform-ops/<tag>/SKILL.md`. On cache
   hit, use it; on miss + offline, fail with a clear message.

6. Generate `.usezombie/platform-ops/SKILL.md` in the user's repo. Substitute:
   - `{{slack_channel}}` → user's value
   - `{{prod_branch_glob}}` → user's value
   - `{{cron_opt_in}}` → user's choice (if true, add the cron block to the
     `x-usezombie.trigger.cron:` section; if false, omit)
   - `{{repo}}` → repo name from git remote
   - `{{model}}` and `{{context_cap_tokens}}` per Discovery D2: this skill is the **platform-managed** install path, so it GETs `https://api.usezombie.com/_um/.../model-caps.json` once for the platform default model (e.g. `claude-sonnet-4-6`) and writes the resolved cap (e.g. `200000`) into `x-usezombie.context.context_cap_tokens`. Current `main` does not yet drive BYOK installs through this skill; any tenant-scoped `provider set` flow remains the pending M48 target contract.

7. If the directory `.usezombie/platform-ops/` already exists in the user's repo,
   refuse to overwrite without `--force`. Ask the user: "Existing
   `.usezombie/platform-ops/` found. Overwrite? (y/N)". On no, stop.

8. Run `zombiectl install --from .usezombie/platform-ops/`. Capture the
   returned zombie id.

9. Run `zombiectl steer {id} "morning health check"` and stream the response
   inline.

10. Print: "Platform-ops zombie installed (id: {id}). It now watches your GH
    Actions CD pipeline. Configure your repo's webhook to point at:
    POST https://api.usezombie.com/v1/webhooks/{zombie_id}
    with secret: <one-time displayed value; already stored in this workspace's
    vault as the `github` credential, field `webhook_secret`>. To steer manually
    any time: `zombiectl steer {id} \"<message>\"`. To kill: `zombiectl kill {id}`."

## Failure modes

If any step fails, print the exact error and stop. Do not silently retry. Do not
half-install. Examples:
- `zombiectl doctor` says auth missing → "Run `zombiectl auth login` first."
- Fetch fails + cache miss → "Cannot fetch template. Try again with internet."
- `op` errors with auth → "Run `op signin` first."

## Out of scope

Non-GitHub CI providers (GitLab, CircleCI, Jenkins) — future version.
Bash one-liner installer — use this skill instead; it's portable across all
agent CLIs that read SKILL.md.
```

### §4 — Repo detection logic

The skill's body has natural-language detection logic. The agent (Claude Code etc.) reads `.github/workflows/*.yml`, `fly.toml`, etc. via its file-read tool. The skill doesn't need a separate "detector binary" — the LLM is the detector, given the patterns to look for.

> **Implementation default:** if multiple workflow files exist, ask the user which one is the production deploy workflow. Default to the file with `deploy` in its name; if multiple, prompt.

### §5 — Credential resolution order

The skill's body specifies the resolution order. The agent runs the commands. No new code in the runtime — this is pure SKILL.md prose driving existing CLI commands.

### §6 — Template fetch + cache

Fetch URL from frontmatter `template_url`, substitute `{tag}`. The agent uses its HTTP/fetch tool. Cache to `~/.cache/usezombie/skills/usezombie-install-platform-ops/<tag>/`. Implementation default: cache key is the tag — bumping the tag invalidates older caches automatically.

### §7 — usezombie.sh CDN serving

Out of scope here (covered by M51). The skill is fetchable from raw GitHub URL on day 1; usezombie.sh CDN is a follow-up convenience.

---

## Interfaces

```
Skill invocation (same name in every host):
  Claude Code / Amp / Codex CLI / OpenCode:   /usezombie-install-platform-ops

Skill input (variables, resolved per host):
  slack_channel: string (required)
  prod_branch_glob: string (default "main")
  cron_opt_in: boolean (default false)
  # Current `main` keeps BYOK credential setup outside the install skill through
  # the workspace-scoped `credentials/llm` route. Tenant-scoped `provider set`
  # remains the pending M48 target contract.

Skill output (filesystem state after success):
  .usezombie/platform-ops/SKILL.md created in user's CWD
  Zombie installed in user's tenant/workspace via zombiectl
  Vault populated with credentials referenced by the skill
  zombiectl steer round-trip printed to stdout

Eval contract (usezombie/skills-evals):
  Per fixture repo: assert skill produced expected substituted SKILL.md
  Per skill: LLM-judge prose clarity score >= 7/10 over 5 trials
```

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| `zombiectl doctor` reports `auth_token_present: false` | User not logged in | Skill prints the doctor JSON, hints `zombiectl auth login`, exits |
| Repo lacks `.github/workflows/` | Non-GH-CI repo | Skill stops with "GH Actions only in v1" message |
| `.usezombie/platform-ops/` exists | Re-running on same repo | Skill prompts overwrite (y/N); on N, exits cleanly |
| Template fetch fails + no cache | Offline | Skill prints clear "no template, try online" message |
| `zombiectl credential add` fails | API down or auth expired | Skill captures stderr, surfaces verbatim, exits |
| Steer round-trip times out (>60s) | Worker not picking up event | Skill prints "zombie installed but first response slow — check `zombiectl events {id}`" |
| Variable resolution: user provides invalid value (e.g., empty Slack channel) | User typo | Skill re-prompts up to 2x; on 3rd empty, exits with hint |

---

## Invariants

1. **Skill never runs `zombiectl install` until `zombiectl doctor` passes.** Hard precondition.
2. **Skill never overwrites `.usezombie/platform-ops/` without explicit consent.** Refuse without `--force`.
3. **Skill is host-neutral.** Variables resolved via the host's primitive, OR fall back to inline natural-language prompts. NEVER hard-codes Claude Code's AskUserQuestion.
4. **Templates are version-pinned.** Skill never fetches `main` — always a pinned tag. Tag bumps are explicit skill releases.

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
- [ ] All 11 tests pass (10 functional + 1 eval-suite)
- [ ] Manual: Customer Zero (author) runs `/usezombie-install-platform-ops` on the usezombie repo itself, ends with a working zombie posting to author's Slack
- [ ] Manual: same author runs `/usezombie-install-platform-ops` in at least one non-Claude host (Amp, Codex CLI, or OpenCode), produces byte-identical `.usezombie/platform-ops/SKILL.md`
- [ ] Skill is fetchable from `https://raw.githubusercontent.com/usezombie/skills/main/usezombie-install-platform-ops/SKILL.md` AND (post-M51) `https://usezombie.sh/skills/usezombie-install-platform-ops/SKILL.md`
- [ ] Single SKILL.md directory with no host-specific packaging fork — same drop-in works for every supported host

---

## Out of Scope

- Non-GitHub-Actions CI providers (M{N+}_001)
- Bash one-liner installer (rejected — cross-platform shell-test cost too high)
- Skills for other zombie shapes — separate milestones if/when those shapes ship
- Skill auto-update mechanism (user re-runs skill to get newer template)
- Skill telemetry (covered by M51 install-pingback)

---

## Discovery owed by M49 (locked Apr 29, 2026)

### `context_cap_tokens` source of truth — locked

M41 lands `ContextBudget.context_cap_tokens: u32` as a passthrough wire field on `executor.createExecution`. M41 does **not** know how to populate it. The end-to-end "where does this number come from" question is answered below; the architecture cross-reference now lives in [`docs/architecture/user_flow.md`](../../architecture/user_flow.md) §8.7 and [`docs/architecture/billing_and_byok.md`](../../architecture/billing_and_byok.md) §9.

#### Decision 1 (D1) — model→cap source of truth

**Locked: a hosted endpoint at a cryptic public URL.**

```
GET https://api.usezombie.com/_um/da5b6b3810543fe108d816ee972e4ff8/model-caps.json
GET https://api.usezombie.com/_um/da5b6b3810543fe108d816ee972e4ff8/model-caps.json?model=<urlencoded>
```

Properties (full design in `billing_and_byok.md` §9):
- Cryptic path key (sixty-four bits of entropy) keeps random scanners off the endpoint without making the URL secret. Hard-coded in `zombiectl` and the install-skill; rotated quarterly via a coordinated CLI + skill release.
- Cloudflare-fronted, aggressive caching (`Cache-Control: public, max-age=86400, immutable`), per-IP rate limit of 1 RPS sustained / burst 10.
- Backed by a static JSON file in v2.0. Later, an admin-zombie owned by `nkishore@megam.io` wakes hourly, queries each provider's models endpoint where one exists, reconciles, and opens a PR with deltas. Same endpoint; fresher data. The admin-zombie is a dogfood instance of the platform-ops pattern.
- Resolved exactly once per install (platform-managed) or once per `provider set` (BYOK). **Never resolved at trigger time.**

Rejected: skill-embedded JSON (forces a skill release per cap change), provider API at install (Anthropic / OpenAI publish models endpoints; Fireworks / Together / Groq don't reliably).

#### Decision 2 (D2) — Bring-Your-Own-Key split

**Locked: same endpoint serves both postures.** The install-skill and `zombiectl provider set` both call `/_um/.../model-caps.json` with the active model name and pin the cap into the appropriate place:

- **Platform-managed.** The install-skill resolves the platform-default model's cap and writes it into the generated `SKILL.md` frontmatter under `x-usezombie.context.context_cap_tokens`.
- **Bring Your Own Key.** `zombiectl provider set` (M48) resolves the cap for the model in the operator's `llm` credential and writes it into `core.tenant_providers.context_cap_tokens`. The install-skill writes `context_cap_tokens: 0` and `model: ""` in the generated frontmatter as sentinels; the worker overlays from `tenant_providers` at trigger time.

The cap is **not** in the `llm` credential body. The body stays `{provider, api_key, model}` — splitting cap from credential lets the cap re-resolve when the model changes without touching the vault.

#### Decision 3 (D3) — architecture cross-reference

**Locked: the architecture doc is split.** architecture/ is now a TOC. The end-to-end walkthrough lives in:

- [`docs/architecture/user_flow.md`](../../architecture/user_flow.md) §8.7 — the three-rail diagram showing platform vs BYOK origin and how the worker overlays sentinels at trigger time.
- [`docs/architecture/billing_and_byok.md`](../../architecture/billing_and_byok.md) §9 — the endpoint shape, rotation, and Cloudflare configuration.
- [`docs/architecture/scenarios/01_default_install.md`](../../architecture/scenarios/01_default_install.md) and [`02_byok.md`](../../architecture/scenarios/02_byok.md) — the two end-to-end scenarios.

These edits already landed in this same Discovery commit. M49 §1 inherits the locked decisions.

### SKILL.md frontmatter shape — what the skill writes

**Platform-managed install** (Scenario 01):

```yaml
x-usezombie:
  model: claude-sonnet-4-6                    # platform default at install time
  context:
    context_cap_tokens: 200000                # ← from /_um/.../model-caps.json
    tool_window: auto
    memory_checkpoint_every: 5
    stage_chunk_threshold: 0.75
```

**Bring-Your-Own-Key install** (Scenario 02):

```yaml
x-usezombie:
  model: ""                                   # sentinel: worker overlays from tenant_providers
  context:
    context_cap_tokens: 0                     # sentinel: worker overlays from tenant_providers
    tool_window: auto
    memory_checkpoint_every: 5
    stage_chunk_threshold: 0.75
```

The worker treats `model == ""` and `context_cap_tokens == 0` as "resolve at trigger time from `tenant_providers`." The two sentinels are independent — either can be populated in frontmatter and overlaid by tenant config or vice versa.

### Why this lands in M49, not M41

The cap question is inseparable from the install-skill's design: whatever decision the skill makes about where models come from also decides where the cap comes from. Solving it inside M41 would force the executor to know about model identity (rejected — that's what made the original `model_registry.zig` rot). Centralising the lookup in M49 + M48 lets the skill and cap design stay coherent.

### Required before §1 starts — done

- ✓ Lookup mechanism locked to the hosted cryptic-URL endpoint.
- ✓ Frontmatter shape sketched (above).
- ✓ Architecture cross-reference landed in `docs/architecture/user_flow.md` §8.7 and `docs/architecture/billing_and_byok.md` §9 in this same commit.
