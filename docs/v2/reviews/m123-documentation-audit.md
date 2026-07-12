# Milestone 123 (M123) documentation audit

**Reviewed:** Jul 12, 2026
**Baseline:** `main` when the M123 branches were created
**Current:** Uncommitted M123 worktrees after verification

## Outcome

The public docs now use 53.6% fewer words. The OpenAPI source uses 15.7% fewer words.

The shorter pages keep commands, limits, error identifiers, and expected output. The error reference grew because each error now has a stable link and a prevention step.

## Published docs

This count excludes `changelog.mdx`. It includes snippet files.

`snippets/rates.mdx` remains a code fragment for rate checks. It is not customer plan copy, so page-shape and prose checks exclude it.

| Measure | Before | After | Change |
| --- | ---: | ---: | ---: |
| Files | 28 | 27 | -3.6% |
| Lines | 3,265 | 2,136 | -34.6% |
| Words | 28,150 | 13,061 | -53.6% |
| Bytes | 193,339 | 88,343 | -54.3% |

| Page | Words before | Words after | Change | Reason |
| --- | ---: | ---: | ---: | --- |
| Quickstart | 1,002 | 443 | -55.8% | Kept one complete first-run path. |
| Command Line Interface (CLI) reference | 3,521 | 1,011 | -71.3% | Removed repeated examples and stale verbs. |
| Connector guide | 1,390 | 303 | -78.2% | Kept customer steps and removed internal details. |
| Troubleshooting | 2,012 | 289 | -85.6% | Kept symptom, cause, fix, and prevention. |
| Fleet authoring | 1,989 | 340 | -82.9% | Kept the required file shape and checks. |
| Error reference | 4,241 | 5,267 | +24.2% | Added stable links, fixes, and prevention for each error. |

## Documentation rule findings

| Check | Before | After |
| --- | ---: | ---: |
| Sentences over 25 words | 74 | 0 |
| Paragraphs over 3 sentences | 45 | 0 |
| Banned words | 14 | 0 |
| Unexpanded abbreviations | 67 | 0 |
| Commands without expected output | 98 | 0 |
| Heading errors | 26 | 0 |
| Missing required page fields | 130 | 0 |
| Invalid date or product metadata | 26 | 0 |

One banned-word finding was marketing copy. The other 13 findings were unclear or internal words.

## OpenAPI

| Measure | Before | After | Change |
| --- | ---: | ---: | ---: |
| Files | 28 | 28 | 0% |
| Lines | 5,321 | 5,136 | -3.5% |
| Words | 15,808 | 13,322 | -15.7% |
| Bytes | 185,948 | 165,958 | -10.7% |

The connector API dropped from 1,067 words to 744 words. That is a 30.3% reduction.

The route check found 67 served `/v1` routes. Ten internal routes stay outside the public description.

All served public routes are documented. The OpenAPI linter still reports 28 warnings that existed before M123.

## Source text

| Check | Before | After |
| --- | ---: | ---: |
| Sentences over 25 words | 25 | 0 |
| Unclear public words | 17 | 0 |
| Removed CLI command forms | 9 | 0 |
| Internal details in public text | 35 | 0 |
| Mutable price prose | 1 | 0 |
| Release-number prose | 1 | 0 |
| Marketing claims | 0 | 0 |

## Quickstart scenario

The quickstart teaches one GitHub review path.

1. You install the CLI.
2. You sign in.
3. You save a GitHub token and a webhook secret.
4. You install `github-pr-reviewer` from the fleet library.
5. You register a GitHub webhook.
6. You open a Pull Request (PR).
7. `agentsfleet` posts review comments.
8. You confirm the run in the activity stream.

The quickstart uses a manual GitHub webhook. It does not claim the shared GitHub App path.

The manual route now accepts supported pull request actions. A clean platform onboarding depends on a valid root bundle in `agentsfleet/github-pr-reviewer`.

That external repository lost its root `SKILL.md` and `TRIGGER.md` when the nested skill directory was deleted upstream. Indy authorized the fix; a PR restoring both files at repo root (matching this monorepo's `tests/fixtures/fleetbundle/github-pr-reviewer` fixture) is open at `agentsfleet/github-pr-reviewer#5`. The quickstart's clean-platform path stays blocked until that PR merges.

## Architecture scenarios

The GitHub review scenario now uses the library name `github-pr-reviewer`. No alias remains in current files.

The production repair scenario covers a failed deploy through a draft PR and health check. Every unproven step is marked as a target, not shipped behavior.

The detailed deploy example moved out of `user_flow.md`. That file dropped from 3,870 words to 3,512 words.

## Fleet bundle test fixture

A test fixture is saved input used by automated tests. `tests/fixtures/fleetbundle/platform-ops/` is test-only input.

The API, user interface (UI), and CLI do not load that directory in production. Acceptance tests upload its skill and trigger through the workspace library API.

The tests then check install, update, run, and delete behavior through the CLI. Removing the fixture would remove that coverage.

The fixture declares `platform-ops-agent`. M123 fixed the test helpers to replace that exact name.

## Verification

| Check | Result |
| --- | --- |
| Docs rule tests | 22 passed |
| Docs site validation | Passed |
| Docs broken links | 0 |
| Product documentation tests | 12 passed |
| Architecture tests | 6 passed |
| CLI coverage run | 1,300 passed, 15 live tests skipped |
| Zig server tests | 1,608 passed, 546 environment tests skipped |
| Full integration suite | Passed with PostgreSQL and Redis |
| Linux builds | Passed for x86_64 and aarch64 |
| Memory leak check | Passed |
| Whole-worktree harness | All checks passed |
| OpenAPI route coverage | 67 served routes covered |
| Product lint bundle | Passed |
| Secret scan | Git history and all changed files passed |

## Pull Request shape

The product PR carries source text, OpenAPI, CLI fixes, tests, architecture, and the product pre-commit check.

The docs PR carries current pages, API navigation, docs tests, and the docs pre-commit check. Historical changelog content is unchanged.
