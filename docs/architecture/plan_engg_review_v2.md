# Engineering Review v2 — Validation Plan

> Parent: [`README.md`](./README.md)
>
> Status: Internal validation note, cleaned for durable reference.
> Use this file for what must be tested around the platform-ops wedge.
> Do not use it as the source of truth for active milestone status or dated launch sequencing.

This file is consumed by `/qa` and `/qa-only` as primary test input.

---

## Purpose

This file defines the durable validation shape for the platform-ops wedge.

It exists to answer:

- what user-facing surfaces must work
- what end-to-end flows must be verified
- what edge cases matter before this wedge is trusted
- what regressions must not be introduced while packaging the install experience

## Surfaces under test

The wedge mainly exercises existing runtime surfaces rather than inventing a completely new subsystem.

- CLI: `zombiectl install --from <path>`
- CLI: `zombiectl steer <zombie_id> <message>` (post-M69_002: enters Read-Eval-Print Loop (REPL) when stdin is a Terminal (TTY) and no message arg is given)
- CLI: `zombiectl credential add <name> --data @-` (the v2 command surface is `add`, not `set`; verified against `zombiectl/src/program/cli-tree.js:331`)
- CLI: `zombiectl doctor --json`
- CLI: `zombiectl tenant provider set --credential <name>` (the self-managed activation surface)
- Skill: `/usezombie-install-platform-ops`
- Docs surface: `docs.usezombie.com/quickstart/platform-ops`
- Docs surface: `docs.usezombie.com/skills`
- Skill distribution surfaces (two paths by audience, see [`user_flow.md`](./user_flow.md) §8.0):
  - Humans: `https://usezombie.sh/skills.md` (copy-paste install)
  - Agents: `npm install -g @usezombie/zombiectl && npx skills add usezombie/skills` (post-M69_001)

## Core happy path

The core happy path is:

1. The user runs `/usezombie-install-platform-ops` inside a target repo.
2. The skill detects the repo shape and asks a small number of gating questions.
3. The skill resolves credentials in the expected order (1Password CLI → environment variables → interactive prompt).
4. The skill writes `.usezombie/platform-ops/SKILL.md` and `.usezombie/platform-ops/TRIGGER.md`.
5. The skill calls `zombiectl doctor --json` and gets a clean readiness result — the only sanctioned preflight surface (see [`user_flow.md`](./user_flow.md) §8.2).
6. The skill calls `zombiectl install --from .usezombie/platform-ops/`.
7. The skill runs `zombiectl steer <zombie_id> "morning health check"` for a real smoke test.
8. The smoke-test message produces a real diagnosis and a Slack post.

If this path fails, the wedge is not ready no matter how polished the surrounding docs are.

**Launch contract on self-managed:** activation flows through the **tenant-scoped** provider configuration (`zombiectl tenant provider set --credential <name>`). The earlier workspace-scoped `/credentials/llm` route was removed before v2.0.0; it must not be preserved as a compatibility path (RULE NLG — pre-`2.0.0`, no legacy framing or compat shims).

## Hosted posture assumptions

v2 assumes the hosted path.

That means validation should confirm:

- the skill defaults to `api.usezombie.com`
- auth repair paths are clear when the CLI is unauthenticated
- the skill does not expose a self-host branch in v2
- any `/self-host` public doc route remains intentionally absent

## Critical paths

These are the must-pass end-to-end flows:

- cold install on a real repo that matches the wedge
- first steer after install
- first real webhook-triggered failure after install
- re-run idempotency on the same repo
- self-managed provider key resolution on at least one real provider path
- gitleaks-clean generated files

The most important rule is that the first steer and the first webhook must hit the same reasoning loop. If those paths diverge in meaningful behaviour, the wedge becomes much harder to explain and trust.

## Edge cases worth keeping

These edge cases are durable and should remain part of validation thinking:

### Repo detection

- no `.github/workflows/` present
- multiple workflow files with ambiguous deploy targets
- `fly.toml` present but no GitHub Actions workflow
- monorepo layouts with multiple candidate roots

### Credential resolution

- 1Password CLI present but not signed in
- env var present but empty
- interactive prompt receives empty input repeatedly
- credential shape is syntactically accepted but operationally weak

### File generation

- target directory already exists
- user is inside a git worktree
- generated file accidentally contains token-shaped content
- template fetch or cache behavior under degraded network conditions

### Install and steer

- `zombiectl install` exits non-zero
- operator interrupts the interactive steer session
- operator re-runs the install skill after success

## Regression surface

Packaging the install experience must not silently break existing substrate behaviour.

These should be treated as regression-sensitive:

- `zombiectl install --from <path>`
- `zombiectl credential set`
- `zombiectl list`
- `zombiectl status`
- `zombiectl kill`
- `zombiectl logs`
- `zombiectl doctor`
- the existing platform-ops sample inputs
- `core.zombie_events` semantics

The install skill should consume platform-ops inputs, not mutate the sample definition into a special-case fork.

## Test infrastructure expectations

The validation shape assumes:

- a fixture repo for deterministic install tests
- CI coverage for generated-file assertions
- gitleaks scanning on generated artifacts
- at least one realistic end-to-end smoke test against the hosted runtime
- a repeatable evaluation set for diagnosis quality

The exact repo layout can evolve. The requirement is that the validation remain reproducible and not depend entirely on one manual founder run.

## Quality bar

The wedge is ready only if:

- the install flow is fast and deterministic
- the first steer proves real evidence gathering
- the webhook path proves the same reasoning loop under production-style input
- generated files are safe to commit
- self-managed does not feel like a second-class path
- failure messages are clear enough that an operator can recover without reading source code

## How to use this file

Use this file when writing:

- QA plans
- release-readiness criteria for the platform-ops wedge
- integration-test coverage plans
- docs that explain what the install flow should prove

Do not use this file as:

- a snapshot of milestone status
- a dated launch checklist
- a source of product positioning

For product positioning, use [`office_hours_v2.md`](./office_hours_v2.md) plus the core architecture files.
