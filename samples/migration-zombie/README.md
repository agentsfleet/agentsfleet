---
name: migration-zombie
version: 0.1.0
description: Mechanical codebase migrations. Long-running, produces a PR you can review in one sitting.
status: experimental
tags: [dev, coding, migrations]
requires:
  skills:
    - git-readwrite
    - shell-sandboxed
    - github-pr
  credentials:
    - github
  worker:
    placement: cloud
    min_memory: 2Gi
---

# Migration Zombie

Mechanical code migrations, run autonomously overnight. Bump Node 18 → 22.
Convert CommonJS → ESM. Move from Express to Hono. Mongoose to Drizzle.
Jest to Vitest.

Point it at a repo with a migration spec, go to sleep, wake up to a PR.

## What it does

- Loads a migration playbook (built-in or user-provided)
- Explores the repo, enumerates every file that needs changing
- Plans the migration in dependency order
- Makes changes, runs the test suite after each step, rolls back if tests break
- Iterates until done or the step budget is exhausted
- Opens a PR with a clean commit history (one commit per migration step)

## Built-in playbooks

- `node-major-bump` — Node 18→20, 20→22. Handles common breakage.
- `cjs-to-esm` — CommonJS to ESM migration
- `jest-to-vitest` — Testing framework swap
- `express-to-hono` — HTTP framework port
- `mongoose-to-drizzle` — ORM swap (schema-preserving)
- `cra-to-vite` — Create React App to Vite

Custom playbooks can be written as a single YAML file describing:
replacements, import rewrites, test-pass criteria.

## Why this isn't just Devin

Devin is a generalist. Migration Zombie is narrow and structured:

- Playbook-driven — step-by-step, each step individually verifiable
- Runs to completion or stops cleanly — no "mostly done" PRs
- Pricing is flat ($29/migration), not per-token
- Can be run locally on your own runner, keeping the code on your box

## Example

    zombie migrate --playbook jest-to-vitest github.com/me/my-app

    [00:00] Loaded playbook: jest-to-vitest (v1.3.0)
    [00:01] Analysing repo... 47 test files, 3 jest configs
    [00:04] Plan:
              1. Swap jest for vitest in package.json
              2. Rewrite jest.config.* to vitest.config.ts
              3. Rewrite test imports (jest → vitest)
              4. Fix snapshot format differences
              5. Run full suite
    [00:06] Step 1/5 ... ok
    ...
    [01:42] Full suite passes. Opening PR.
