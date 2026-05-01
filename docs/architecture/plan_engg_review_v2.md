# Engineering Review v2 — Validation Plan

> Parent: [`README.md`](./README.md)
>
> Status: Internal validation note, cleaned for durable reference.
> Use this file for what must be tested around the platform-ops wedge.
> Do not use it as the source of truth for active milestone status or dated launch sequencing.

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
- CLI: `zombiectl steer {id}`
- CLI: `zombiectl credential add ...`
- CLI: `zombiectl doctor --json`
- Skill: `/usezombie-install-platform-ops`
- Docs surface: `docs.usezombie.com/quickstart/platform-ops`
- Docs surface: `docs.usezombie.com/skills`
- Skill distribution surface: `https://usezombie.sh/skills.md`

## Core happy path

The core happy path is:

1. The user runs `/usezombie-install-platform-ops` inside a target repo.
2. The skill detects the repo shape and asks a small number of gating questions.
3. The skill resolves credentials in the expected order.
4. The skill writes `.usezombie/platform-ops/{SKILL,TRIGGER,README}.md`.
5. The skill calls `zombiectl doctor --json` and gets a clean readiness result.
6. The skill calls `zombiectl install --from .usezombie/platform-ops/`.
7. The skill opens `zombiectl steer {id}`.
8. A real "morning health check" message produces a real diagnosis and Slack post.

If this path fails, the wedge is not ready no matter how polished the surrounding docs are.

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
- Bring Your Own Key resolution on at least one real provider path
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
- `zombiectl credential add`
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
- BYOK does not feel like a second-class path
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
