# M67_001: Oxlint replaces ESLint for JavaScript and TypeScript surfaces

**Prototype:** v2.0.0
**Milestone:** M67
**Workstream:** 001
**Date:** May 10, 2026
**Status:** PENDING
**Priority:** P2 — developer tooling migration that removes duplicated ESLint setup from CLI and UI packages.
**Categories:** CLI, UI
**Batch:** B1 — independent tooling workstream
**Branch:** feat/m67-oxlint-migration
**Depends on:** N/A — no active workstream dependency.

**Canonical architecture:** `docs/architecture/user_flow.md` §8.2 for `zombiectl` as the local operator surface; `docs/architecture/data_flow.md` §D for UI visibility surfaces. This work changes only repository lint tooling.

---

## Implementing agent — read these first

1. `docs/greptile-learnings/RULES.md` — universal JavaScript and TypeScript rules, especially RULE NDC, RULE UFS, RULE ORP, and RULE TST-NAM.
2. `docs/BUN_RULES.md` — TypeScript/Bun editing discipline and package script expectations.
3. `zombiectl/eslint.config.js` — current CLI lint target shape and existing rule intent.
4. `ui/packages/app/eslint.config.js` — current Next.js app lint targets and typed rule intent.
5. `ui/packages/website/eslint.config.js` and `ui/packages/design-system/eslint.config.js` — current Vite and design-system lint target shape.
6. <https://oxc.rs/docs/guide/usage/linter/migrate-from-eslint.html> — official Oxlint migration guidance.

---

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — universal source discipline; RULE ORP applies to deleting ESLint configs and dependencies.
- **`docs/BUN_RULES.md`** — package script and JavaScript/TypeScript tooling changes.
- **`docs/TEMPLATE.md`** — spec authoring prohibitions.
- **`docs/gates/spec-template.md`** — SPEC TEMPLATE GATE for this spec.

---

## Anti-Patterns to Avoid

1. Keeping ESLint configs as inactive fallback files. Deleted tooling must be removed cleanly.
2. Disabling all Oxlint rules to make the migration pass. The baseline must preserve correctness checks from the provided Bun profile.
3. Treating framework-specific gaps as invisible. Any rule coverage dropped from ESLint must be named in Discovery or retained through another command such as `tsc`.
4. Editing application behavior to appease a tooling migration unless the lint finding exposes a real bug.

---

## Overview

**Goal (testable):** `zombiectl`, app, website, and design-system lint commands run Oxlint instead of ESLint and pass from their package roots.

**Problem:** The repository carries separate ESLint configs and dependency sets across the CLI and UI packages. The Captain wants the Bun-style Oxlint profile used instead, so linting is faster and the rule baseline is expressed in Oxlint configuration.

**Solution summary:** Add Oxlint configuration for each JavaScript/TypeScript package surface, switch package lint scripts from ESLint to Oxlint, remove ESLint-only configs and dependencies, and verify lint/type/test commands that cover the migrated packages.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M67_001_P2_CLI_UI_OXLINT_MIGRATION.md` | CREATE | Tracks this tooling migration as one workstream. |
| `docs/v2/active/M67_001_P2_CLI_UI_OXLINT_MIGRATION.md` | MOVE/EDIT | CHORE(open) lifecycle updates status and branch. |
| `docs/v2/done/M67_001_P2_CLI_UI_OXLINT_MIGRATION.md` | MOVE/EDIT | CHORE(close) lifecycle records completion. |
| `zombiectl/.oxlintrc.json` | CREATE | Oxlint config for the CLI package. |
| `zombiectl/eslint.config.js` | DELETE | ESLint config removed after replacement. |
| `zombiectl/package.json` | EDIT | Lint scripts and dev dependencies switch to Oxlint. |
| `zombiectl/bun.lock` | EDIT | Package manager lockfile reflects dependency migration. |
| `ui/packages/app/.oxlintrc.json` | CREATE | Oxlint config for the dashboard app. |
| `ui/packages/app/eslint.config.js` | DELETE | ESLint config removed after replacement. |
| `ui/packages/app/package.json` | EDIT | Lint script and dev dependencies switch to Oxlint. |
| `ui/packages/website/.oxlintrc.json` | CREATE | Oxlint config for the website package. |
| `ui/packages/website/eslint.config.js` | DELETE | ESLint config removed after replacement. |
| `ui/packages/website/package.json` | EDIT | Lint scripts and dev dependencies switch to Oxlint. |
| `ui/packages/design-system/.oxlintrc.json` | CREATE | Oxlint config for shared UI primitives. |
| `ui/packages/design-system/eslint.config.js` | DELETE | ESLint config removed after replacement. |
| `ui/packages/design-system/package.json` | EDIT | Lint script and dev dependencies switch to Oxlint. |
| `bun.lock` | EDIT | Workspace lockfile reflects UI package dependency migration. |

---

## Sections (implementation slices)

### §1 — Spec and lifecycle setup

The migration lands behind one spec, then moves to `active/` on the implementation branch before package files change.

### §2 — Oxlint configuration

Each package gets an Oxlint config based on the Bun profile provided by the Captain. Package-specific ignore patterns and overrides keep fixture and generated files out of the lint path.

### §3 — Script and dependency migration

Package lint scripts invoke Oxlint. ESLint packages and plugin dependencies are removed when no longer referenced. Existing typecheck commands remain responsible for TypeScript-only guarantees that Oxlint does not replace.

### §4 — Verification and cleanup

Run lint from each migrated package plus the repository verification commands that apply to JavaScript/TypeScript-only tooling changes. Delete stale ESLint configs and confirm no source or package script still invokes ESLint.

---

## Interfaces

Package scripts are the public developer interface for this workstream:

- `zombiectl`: `bun run lint`, `bun run lint:fix`
- `ui/packages/app`: `bun run lint`
- `ui/packages/website`: `bun run lint`, `bun run lint:fix`
- `ui/packages/design-system`: `bun run lint`

No HTTP API, schema, `zombiectl` user command, or runtime interface changes.

---

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Unsupported ESLint rule | Oxlint lacks an exact equivalent for a previous plugin rule | Preserve the high-value rule via an Oxlint-supported equivalent, typecheck command, or Discovery note. |
| Fixture parse failure | Oxlint parses intentional invalid JavaScript or TypeScript fixtures | Add package-local ignore patterns for those fixtures only. |
| Dependency drift | ESLint dependency remains after config deletion | Search package manifests and lockfiles for ESLint references before close. |
| Type-only gap | ESLint typed rules previously caught an issue Oxlint does not cover | Keep `tsc` or existing typecheck scripts in verification and lint chains where already present. |

---

## Invariants

1. No package lint script invokes `eslint` after the migration — enforced by `rg -n "eslint" zombiectl ui package.json`.
2. Every package that deletes `eslint.config.js` has a replacement `.oxlintrc.json` — enforced by file presence and package lint commands.
3. TypeScript packages keep type checking available through existing `typecheck`, `build`, or `tsc --noEmit` scripts — enforced by package scripts.
4. No application runtime behavior changes — enforced by limiting edits to tooling config, manifests, lockfiles, and spec lifecycle files.

---

## Test Specification

| Test | Asserts |
|------|---------|
| `zombiectl lint uses Oxlint` | `bun run lint` in `zombiectl/` invokes Oxlint and exits cleanly. |
| `zombiectl lint fix command parses` | `bun run lint:fix` in `zombiectl/` invokes Oxlint with fix mode without missing binary errors. |
| `app lint uses Oxlint` | `bun run lint` in `ui/packages/app/` invokes Oxlint and exits cleanly. |
| `website lint uses Oxlint` | `bun run lint` in `ui/packages/website/` invokes Oxlint and exits cleanly. |
| `website lint fix command parses` | `bun run lint:fix` in `ui/packages/website/` invokes Oxlint with fix mode without missing binary errors. |
| `design-system lint uses Oxlint` | `bun run lint` in `ui/packages/design-system/` invokes Oxlint and preserves type checking. |
| `eslint references removed` | Repository search finds no active ESLint configs, scripts, or dependencies under migrated surfaces. |

---

## Acceptance Criteria

1. `cd zombiectl && bun run lint` exits cleanly.
2. `cd ui/packages/app && bun run lint` exits cleanly.
3. `cd ui/packages/website && bun run lint` exits cleanly.
4. `cd ui/packages/design-system && bun run lint` exits cleanly.
5. `rg -n "eslint" zombiectl ui package.json` returns no active lint script, config, or dependency references for migrated surfaces.
6. `make lint` exits cleanly or any environment limitation is documented in the final notes.
7. `make test` exits cleanly or any environment limitation is documented in the final notes.

---

## Discovery (consult log)

Empty at creation.
