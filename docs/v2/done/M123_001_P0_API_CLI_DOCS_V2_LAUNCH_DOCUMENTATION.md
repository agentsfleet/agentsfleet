<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill; zero template guidance or unfilled slots may remain.
- No time or effort estimates. Priority is the sizing signal.
-->

# M123_001: Launch documentation matches the shipped product

**Prototype:** v2.0.0
**Milestone:** M123
**Workstream:** 001
**Date:** Jul 11, 2026
**Status:** DONE
**Priority:** P0 — users must not launch with commands or API guidance that the product rejects
**Categories:** API, CLI, DOCS
**Batch:** B1 — one cross-repository launch-readiness outcome
**Branch:** `chore/m123-m133-close`
**Test Baseline:** unit=2526 integration=307
**Depends on:** none
**Provenance:** agent-generated from Indy's Jul 11, 2026 launch direction
**Canonical architecture:** `docs/architecture/user_flow.md` §8

---

## Overview

**Goal (testable):** A first-day user can follow every published page and CLI example without meeting a removed command, missing API operation, or unexplained failure.
**Problem:** The docs contain rejected CLI verbs, missing API navigation entries, long explanations, and commands without expected output. `agentsfleet --help` also prints a telemetry timeout after valid help.
**Solution summary:** Rewrite the current docs from Zig, OpenAPI, CLI help, and architecture. Add pre-commit checks. Fix confirmed source mismatches, including signed GitHub PR event intake.

## PR Intent & comprehension handshake

- **Pull Request (PR) title:** Align launch docs, API, and CLI with shipped behavior
- **Intent:** A new user gets short, accurate instructions that match the product they run.
- **Handshake** — Orly restates the intent at PLAN and records assumptions before the first edit.

## Implementing agent — read these first

1. `docs/architecture/user_flow.md` — canonical user actions and surface order.
2. `src/agentsfleetd/http/routes.zig` and `public/openapi/` — served API and its public description.
3. `cli/src/program/cli-tree.ts` and sibling tree files — canonical CLI command shape.
4. `~/Projects/dotfiles/docs/DOCUMENTATION_RULES.md` — Constitution and repository adaptations once created.
5. `~/Projects/docs/AGENTS.md` — public terminology and content boundaries.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `docs/v2/done/M123_001_P0_API_CLI_DOCS_V2_LAUNCH_DOCUMENTATION.md` | EDIT | Track proof and close the workstream. |
| `.githooks/pre-commit` | EDIT | Run API and documentation checks before commit. |
| `make/quality.mk` | EDIT | Add the documentation rule check to the existing quality surface. |
| `scripts/check_documentation_rules.py` | CREATE | Check OpenAPI and CLI customer text. |
| `scripts/check_documentation_rules_test.py` | CREATE | Prove every check accepts and rejects the intended cases. |
| `docs/architecture/README.md` | EDIT | Use the canonical `github-pr-reviewer` scenario name and list the approved repair scenario. |
| `docs/architecture/capabilities.md` | EDIT | Record the file and Git tools needed by the repair target. |
| `docs/architecture/user_flow.md` | EDIT | Replace the long deployment example with a short link to the full scenario. |
| `docs/architecture/scenarios/README.md` | EDIT | Index the approved repair scenario and state its proof status. |
| `docs/architecture/scenarios/github-pr-reviewer.md` | RENAME | Replace the shortened filename with the library identifier. |
| `docs/architecture/scenarios/production-deploy-repair.md` | CREATE | Separate the proven diagnosis path from the unproven repair path. |
| `docs/v2/reviews/m102-doc-shape-review.md` | EDIT | Update link-only references after the scenario rename. |
| `docs/v2/reviews/m123-documentation-audit.md` | CREATE | Record reproducible before-and-after metrics and scenario findings. |
| `docs/v2/done/M102_001_P1_API_CLI_UI_AGENT_IDENTITY_PROXY_GITHUB_APP.md` | EDIT | Update link-only references after the scenario rename. |
| `docs/v2/done/M102_005_P1_API_CLI_DOCS_AGENT_IDENTITY_PROXY_TAIL.md` | EDIT | Update link-only references after the scenario rename. |
| `docs/v2/done/M103_001_P1_API_DOCS_INFRA_TWO_TIER_TEMPLATE_CATALOG_ONBOARDING.md` | EDIT | Update link-only references after the scenario rename. |
| `docs/v2/done/M96_001_P2_API_DOCS_INFRA_BUNDLE_R2_CANONICAL_STORE.md` | EDIT | Update link-only references after the scenario rename. |
| `public/openapi/root.yaml` | EDIT | Use plain public descriptions. |
| `public/openapi/components/schemas.yaml` | EDIT | Keep shared response descriptions and examples plain. |
| `public/openapi/paths/*.yaml` | EDIT | Remove stale commands and unclear API prose found by the audit. |
| `public/openapi.json` | EDIT | Regenerate the bundled API file. |
| `build.zig` | EDIT | Pass the repository version to the error-page generator. |
| `src/agentsfleetd/errors/gen_error_codes.zig` | EDIT | Generate a plain reference page with stable error anchors. |
| `src/agentsfleetd/errors/gen_error_codes_test.zig` | EDIT | Pin the page shape and one anchor for every registered error. |
| `src/agentsfleetd/fleet_runtime/frontmatter_fixtures_test.zig` | EDIT | Mark the platform-operations bundle as test input and pin its shared name. |
| `src/agentsfleetd/fleet_bundle/importer.zig` | EDIT | Keep importer wording aligned with fleet bundle names. |
| `src/agentsfleetd/http/handlers/webhooks/github*.zig` | EDIT | Accept supported GitHub PR actions on the signed fleet webhook. |
| `src/agentsfleetd/http/routes.zig` | EDIT | Route GitHub webhook events through the source-backed filter. |
| `src/agentsfleetd/http/webhook_http_integration_test.zig` | EDIT | Prove signed GitHub PR delivery against PostgreSQL and Redis. |
| `schema/023_fleet_library.sql` | EDIT | Seed only the supported `http_request` tool for first-party fleets. |
| `cli/src/services/telemetry/analytics.layer.ts` | EDIT | Keep telemetry shutdown failures out of valid help output. |
| `cli/src/program/cli-tree.ts` | EDIT | Describe local workspace removal accurately. |
| `cli/src/commands/workspace.ts` | EDIT | Make local-only workspace removal clear in output. |
| `cli/src/commands/fleet.ts` | EDIT | Replace the removed local-install hint. |
| `cli/test/telemetry/analytics.layer.unit.test.ts` | EDIT | Prove shutdown remains bounded and silent. |
| `cli/test/telemetry/analytics.layer.fixture.ts` | CREATE | Isolate the telemetry client test helper from the test cases. |
| `cli/test/template-ops.unit.test.ts` | CREATE | Prove both fixture files receive the caller's unique fleet name. |
| `cli/test/acceptance/help-and-errors.spec.ts` | EDIT | Exercise help without hiding telemetry failures. |
| `cli/test/acceptance/workspace-mutation.spec.ts` | EDIT | Prove workspace removal does not claim remote deletion. |
| `cli/test/acceptance/fixtures/constants.ts` | EDIT | Name the shared platform-operations fixture identifier once. |
| `cli/test/acceptance/fixtures/template-ops.ts` | EDIT | Replace the fixture's declared name when creating an acceptance fleet. |
| `cli/test/acceptance/fixtures/update-delete-ops.ts` | EDIT | Reuse the declared fixture name when creating an update bundle. |
| `cli/test/acceptance/fixtures/workspace-ops.ts` | EDIT | Pin the local-removal JSON field. |
| `cli/test/cli-tree-help.unit.test.ts` | EDIT | Pin local-only workspace help. |
| `cli/test/fleet.integration.test.ts` | EDIT | Pin the current library install hint. |
| `cli/test/workspace-effect.unit.test.ts` | EDIT | Pin local-only workspace output. |
| `cli/test/acceptance/fixtures/*.ts` | EDIT | Keep acceptance bundles and command expectations aligned with the CLI. |
| `cli/test/golden/help-no-color.txt` | EDIT | Pin the current help shape within 80 columns. |
| `src/agentsfleetd/errors/error_entries.zig` | EDIT | Replace removed CLI verbs in user-facing error hints. |
| `src/agentsfleetd/errors/error_entries_runtime.zig` | EDIT | Keep generated public error text within the documentation rules. |
| `src/agentsfleetd/fleet/context_resolve.zig` | EDIT | Remove a stale command reference. |
| `src/agentsfleetd/http/handlers/workspaces/lifecycle.zig` | EDIT | Remove a stale command reference. |
| `ui/packages/app/lib/api/workspaces.ts` | EDIT | Remove a stale command reference. |
| `ui/packages/app/tests/e2e/acceptance/install-fleet-cli.spec.ts` | EDIT | Test the shipped library-install path. |
| `ui/packages/app/tests/*install*.test.ts` | EDIT | Keep fleet library examples on supported tool names. |
| `tests/fixtures/fleetbundle/{github-pr-reviewer,security-reviewer,zoho-sprint-daily-summarizer}/*` | EDIT | Use the shipped `http_request` tool in first-party bundle fixtures. |
| `tests/fixtures/fleetbundle/platform-ops/README.md` | EDIT | Stop teaching a rejected install flag. |
| `~/Projects/docs/.githooks/pre-commit` | CREATE | Run the docs checks before commit. |
| `~/Projects/docs/Makefile` | EDIT | Add the Constitution check to `make test` and `make lint`. |
| `~/Projects/docs/README.md` | EDIT | Tell contributors how to enable the tracked pre-commit hook. |
| `~/Projects/docs/scripts/check-documentation.py` | CREATE | Enforce page metadata, skeleton, language, examples, and headings. |
| `~/Projects/docs/scripts/test-documentation.py` | CREATE | Prove the docs linter's pass and failure cases. |
| `~/Projects/docs/scripts/test-spec.sh` | DELETE | Remove the unused checker replaced by the documentation suite. |
| `~/Projects/docs/docs.json` | EDIT | Add every public OpenAPI operation to navigation. |
| `~/Projects/docs/**/*.mdx` | EDIT | Rewrite current pages; exclude `changelog.mdx` and page-only rules for snippets. |
| `~/Projects/dotfiles/docs/DOCUMENTATION_RULES.md` | CREATE | Store the durable documentation Constitution. |
| `~/Projects/dotfiles/dispatch/write_documentation.md` | CREATE | Route documentation edits through the rule. |
| `~/Projects/dotfiles/AGENTS.md` | EDIT | Add the documentation dispatch to the operating model. |
| `~/Projects/dotfiles/docs/EXECUTE_DOC_READS.md` | EDIT | Require the Constitution before relevant edits. |
| `~/Projects/dotfiles/docs/CHANGELOG_VOICE.md` | EDIT | State that the Constitution is read first. |
| `~/Projects/dotfiles/docs/DISPATCH_ARCHITECTURE.md` | EDIT | State that documentation rules govern dispatch prose. |
| `~/Projects/dotfiles/audits/agents-md.md` | EDIT | Add comprehension questions for the new rule. |
| `~/Projects/dotfiles/audits/data.sh` | EDIT | Register the new dispatch pair. |
| `~/Projects/dotfiles/bin/link-agents-md` | EDIT | Ship the new rule to product repositories. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — RULE NDC, NLR, NLG, UFS, TST-NAM, ORP, FLL, TSC, TSJ, XCC.
- **`dispatch/write_zig.md`** — Zig comments and user-facing error strings still require Zig, public-surface, length, and cross-compile checks.
- **`dispatch/write_ts_adhere_bun.md`** — CLI and dashboard TypeScript follow the existing file-shape and Bun rules.
- **`docs/REST_API_DESIGN_GUIDELINES.md`** — OpenAPI remains aligned with served route shape and error responses.
- **`dispatch/edit_rules.md`** — the new dotfiles rule and dispatch require the full invariance audit.
- **`~/Projects/docs/AGENTS.md`** — current public terminology and content boundaries remain binding.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | yes | Edit webhook behavior, generated references, tests, strings, and comments; run both Linux cross-compiles. |
| PUB / Struct-Shape | yes | Review the public webhook event constants and keep their file local. |
| File & Function Length | yes | Keep new checkers split by one purpose per function and under repository caps. |
| UFS | yes | Reuse command and telemetry constants; do not duplicate customer strings. |
| UI Substitution / DESIGN TOKEN | no | No rendered component changes. |
| LOGGING / LIFECYCLE / ERROR REGISTRY / SCHEMA | yes | Preserve stable errors and update migration 023 in the teardown-and-rebuild baseline. |
| SPEC TEMPLATE | yes | Keep every required section complete and under 320 lines. |
| Invariance Suite | yes | Run the script, answer every question, commit, and write the matching sign-off. |

## Prior-Art / Reference Implementations

- **CLI:** `cli/src/program/cli-tree*.ts` and `cli/test/acceptance/` — command tree, output service, and subprocess proof.
- **API:** `docs/REST_API_DESIGN_GUIDELINES.md` with `scripts/check_openapi_route_coverage.py` — Zig routes win and OpenAPI follows.
- **Docs:** Mintlify page components already used in `quickstart.mdx`; keep only components that make a procedure easier to scan.

## Sections (implementation slices)

### §1 — Durable documentation rules — **DONE**

The Constitution becomes a repository rule with deterministic checks before commit.

- **Dimension 1.1 — DONE** — Rules define page types, metadata, language, structure, examples, freshness, and exceptions → Test `test_documentation_rule_sections`
- **Dimension 1.2 — DONE** — Dotfiles routes relevant edits through the rule before narrower voice guides; repository hooks own mechanical checks → Test `test_documentation_dispatch_order`
- **Dimension 1.3 — DONE** — Docs and product pre-commit hooks reject violations → Test `test_precommit_documentation_checks`

### §2 — Source surfaces agree — **DONE**

Zig routes and behavior remain the source of truth. OpenAPI and CLI guidance match them.

- **Dimension 2.1 — DONE** — OpenAPI covers every served public route and uses plain descriptions → Test `test_openapi_public_surface`
- **Dimension 2.2 — DONE** — Docs navigation lists every published OpenAPI operation → Test `test_api_navigation_complete`
- **Dimension 2.3 — DONE** — Live source contains no removed CLI command spelling → Test `test_removed_cli_spelling_absent`
- **Dimension 2.4 — DONE** — Valid CLI help exits cleanly without telemetry noise → Test `test_help_has_no_telemetry_error`

### §3 — Current pages teach one clear path — **DONE**

Every published page gets one page type, short sentences, required sections, concrete examples, and expected output.

- **Dimension 3.1 — DONE** — All current pages pass metadata and skeleton checks → Test `test_current_pages_have_required_shape`
- **Dimension 3.2 — DONE** — CLI and quickstart commands match CLI help and show expected output → Test `test_documented_commands_match_cli`
- **Dimension 3.3 — DONE** — API, concepts, fleet, workspace, billing, and troubleshooting pages use canonical terms and short prose → Test `test_current_pages_plain_language`
- **Dimension 3.4 — DONE** — Changelog history and snippet fragments receive only their applicable checks → Test `test_non_page_exclusions`
- **Dimension 3.5 — DONE** — Architecture scenarios use canonical names and label every unproven repair step → Test `test_architecture_scenarios_match_source`

### §4 — Launch proof and reporting — **DONE**

Both repositories pass their native checks and land as cross-linked PRs.

- **Dimension 4.1 — DONE** — Product checks pass across API, CLI, Zig, and hooks → Test `test_product_verification_bundle`
- **Dimension 4.2 — DONE** — Docs render, links resolve, and the Constitution passes → Test `test_docs_verification_bundle`
- **Dimension 4.3 — DONE** — Review and review polling report zero unresolved launch blockers → Test `test_review_chain_reported`

## Interfaces

```
Page front matter:
type: tutorial | how-to | reference | explanation | troubleshooting
audience: user | operator | contributor
verified: 2026-07-11
product_version: 0.17.0
executable: true | false

Repository outcomes:
agentsfleet PR: source, OpenAPI, CLI, tests, and product pre-commit wiring
docs PR: current MDX pages, API navigation, docs checks, and docs pre-commit wiring
dotfiles master: durable rule, dispatch, audits, and linked delivery
```

## Failure Modes

| Mode | Cause | Handling |
|------|-------|----------|
| Stale command returns | Prose names a removed verb or flag | Pre-commit fails with file, line, and canonical replacement. |
| API navigation drifts | OpenAPI gains or removes an operation | Docs check prints the exact missing or extra operation. |
| Page shape drifts | Metadata or required section is absent | Docs check cites the Constitution rule number. |
| Example cannot run | Command has no expected output or uses an unknown placeholder | Docs check rejects the page before commit. |
| Telemetry pollutes help | Analytics shutdown fails or times out | CLI swallows the telemetry-only failure and the subprocess test asserts empty standard error. |
| Rule wiring drifts | Dispatch, audit list, or doc-read table misses the new rule | Invariance audit blocks the dotfiles commit. |
| Repair scenario overclaims | A target step reads like shipped behavior | The architecture test requires an explicit proof boundary. |

## Invariants

1. Zig route behavior wins over OpenAPI and prose — route coverage and review enforce it.
2. Required page metadata stays consistent — the docs linter enforces one configured value.
3. Changelog history is unchanged — diff check excludes `changelog.mdx`.
4. Snippets are fragments, not pages — page-only checks exclude `snippets/*.mdx`.
5. Every rejection names a Constitution rule number — linter tests enforce the diagnostic prefix.
6. No credential value or internal runner protocol enters customer docs — content checks and review enforce the boundary.
7. The production repair scenario never claims an unproven write path is shipped.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable | not applicable | Documentation is checked locally | File path, rule number, line number | No page content or credentials leave the machine | `test_precommit_documentation_checks` |

Metrics review: no analytics or funnel event changes. Telemetry behavior changes only by suppressing its own shutdown failure after valid CLI output.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts |
|-----------|------|------|---------|
| 1.1 | unit | `test_documentation_rule_sections` | Every Constitution category is present and numbered. |
| 1.2 | unit | `test_documentation_dispatch_order` | General docs rules precede changelog and dispatch-specific guides. |
| 1.3 | integration | `test_precommit_documentation_checks` | Staged invalid fixtures fail and valid fixtures pass. |
| 2.1 | integration | `test_openapi_public_surface` | Bundled OpenAPI matches Zig public routes and passes content checks. |
| 2.2 | unit | `test_api_navigation_complete` | OpenAPI operations and `docs.json` entries are equal. |
| 2.3 | unit | `test_removed_cli_spelling_absent` | Live files contain zero removed command forms. |
| 2.4 | end-to-end | `test_help_has_no_telemetry_error` | Real `agentsfleet --help` exits zero with empty standard error. |
| 3.1 | unit | `test_current_pages_have_required_shape` | Every current page has valid metadata and ordered sections. |
| 3.2 | integration | `test_documented_commands_match_cli` | Commands resolve in the built CLI and each example has output. |
| 3.3 | unit | `test_current_pages_plain_language` | Banned words, long sentences, long paragraphs, and undefined acronyms fail. |
| 3.4 | unit | `test_non_page_exclusions` | Changelog is skipped and snippets avoid page-only checks. |
| 3.5 | unit | `test_architecture_scenarios_match_source` | Scenario names match library identifiers and target repair steps remain marked unproven. |
| 4.1 | integration | `test_product_verification_bundle` | Product lint, unit, API, and cross-compile commands exit zero. |
| 4.2 | integration | `test_docs_verification_bundle` | Docs tests, validation, and links exit zero. |
| 4.3 | end-to-end | `test_review_chain_reported` | Session notes contain all required skill outcomes and no unresolved blocker. |

## Acceptance Rubric (single scoring surface)

| # | Criterion | Verify | Expected | Priority | Graded |
|---|-----------|--------|----------|----------|--------|
| R1 | Current pages pass the Constitution | `make test` in `~/Projects/docs` | exit 0 | P0 | ✅ 22 tests, exit 0 |
| R2 | API navigation equals OpenAPI | `make _lint-openapi-drift` in `~/Projects/docs` | exit 0 with equal operation counts | P0 | ✅ drift check clean |
| R3 | Product public surfaces agree | `make check-openapi` | exit 0 | P0 | ✅ bundle and route checks green |
| R4 | CLI help is clean | `AGENTSFLEET_TELEMETRY_DISABLED=0 bun cli/dist/bin/agentsfleet.js --help >/tmp/m123-help.out 2>/tmp/m123-help.err; test ! -s /tmp/m123-help.err` | exit 0 | P0 | ✅ standard error 0 bytes |
| R5 | Removed command forms are absent | `rg -n 'install --from|workspace add|fleet-key add|secret add|tenant provider add' src cli public ui --glob '!docs/v2/**' --glob '!**/*test*' --glob '!**/tests/**'` | 0 matches | P0 | ✅ 0 live-source matches |
| R6 | Architecture scenario status is honest | `python3 scripts/check_documentation_rules_test.py` | scenario naming and proof-boundary test passes | P0 | ✅ 12 tests, exit 0 |
| S1 | Product unit tests pass | `make test-unit-all` | exit 0 | P0 | ✅ all package lanes passed |
| S2 | Product lint passes | `make lint-all` | exit 0 | P0 | ✅ all lint checks passed |
| S3 | Docs lint passes | `make lint` in `~/Projects/docs` | exit 0 | P0 | ✅ exit 0 |
| S4 | Both Linux targets compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | exit 0 | P0 | ✅ exit 0 |
| S5 | No secrets enter either diff | `gitleaks detect` in each repository | exit 0 | P0 | ✅ no leaks found in both repositories |
| S6 | Diff stays in the listed surface | `git diff --name-only origin/main` | 0 unexplained paths | P0 | ✅ closure records only |

**Grading protocol:** Run every command and record one decisive output line. Every P0 row must pass before close.

## Dead Code Sweep

Delete the unused docs checker replaced by the documentation suite. The orphan sweep checks removed CLI spellings across live source, tests, OpenAPI, architecture, and current docs.

## Out of Scope

- Changing `VERSION`, package versions, or release tags. Indy will change the version separately.
- Rewriting historical changelog entries.
- Documenting internal runner protocol, deployment playbooks, vault paths, or provider account details.
- Adding Continuous Integration checks. M123 uses pre-commit checks by Indy's direction.

## Product Clarity (authoring record)

1. **Successful user moment** — a first-day user copies the quickstart and sees the documented output without correcting a command.
2. **Preserved user behaviour** — current routes, command names, error identifiers, and working examples keep their behavior.
3. **Optimal-way check** — source-derived checks are the shortest path because prose cannot silently outrun the shipped CLI or API.
4. **Rebuild-vs-iterate** — rewrite the customer pages and keep proven runtime behavior; do not redesign the product during launch cleanup.
5. **What we build** — one rules file, pre-commit checks, concise pages, complete API navigation, and source fixes for confirmed public drift.
6. **What we do not build** — no dashboard control, endpoint, command alias, compatibility fallback, or release automation.
7. **Fit with existing features** — the work tightens Mintlify, OpenAPI bundling, CLI help, and existing repository hooks.
8. **Surface order** — CLI and quickstart first, then API, concepts, fleet guides, workspaces, billing, and troubleshooting.
9. **Dashboard restraint** — no dashboard change unless a stale acceptance test names a removed CLI path.
10. **Confused-user next step** — each page ends with a direct verification or common-problem action, not a request to contact support first.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one lead-repository PR corrects source truth and one docs PR rewrites the published site. Dotfiles carries the reusable authoring rule.
- **Alternatives considered:** docs-only edits would leave stale source hints and the noisy help path ready to regress.
- **Patch-vs-refactor verdict:** this is a focused documentation refactor because most pages need the same metadata, structure, and language rules at once.

## Discovery (consult log)

- **Consults** — Indy approved Zig as source of truth and one product branch for source, OpenAPI, and CLI corrections.
- **Pre-commit authorization** — Indy: "I prefer precommit hooks to catch that rather than CI. CI is too far in the cycle." This authorizes M123 edits to the named pre-commit hooks for documentation enforcement.
- **Version decision** — Indy will change the version separately. M123 keeps `0.17.0`.
- **Scenario approval** — Indy approved the production repair scenario under `docs/architecture/scenarios/`.
- **Fixture finding** — `tests/fixtures/fleetbundle/platform-ops/` is acceptance-test input. The API, dashboard, and CLI do not load it in production.
- **Fixture correction** — its declared name is `platform-ops-agent`. Acceptance helpers now replace that exact name.
- **Metrics review** — no analytics or funnel playbook update is required; product event names and properties do not change.
- **Unit-test review** — coverage was added for error generation, telemetry failure, fixture naming, and architecture wiring. The full CLI run passes 1,300 tests.
- **Integration-test review** — the real CLI binary survived a refused telemetry connection three times. It exited zero with empty standard error each time.
- **GitHub event proof** — the signed per-fleet webhook accepts supported PR actions. The full PostgreSQL and Redis integration suite passes.
- **Test delta** — unit=2783 (+257), integration=369 (+62) against the CHORE(open) baseline; the final product unit and integration suites pass.
- **External bundle blocker** — `agentsfleet/github-pr-reviewer` lacks root `SKILL.md` and `TRIGGER.md` files. Clean library onboarding and the published quickstart remain blocked until that repository is corrected.
- **Authority boundary** — Indy authorized the docs and product repositories. Orly requested approval before changing the separate GitHub reviewer repository.
- **External bundle fix authorized** — Indy (Jul 12, 2026): approved fixing `agentsfleet/github-pr-reviewer` directly and reverting the docs' interim `security-reviewer` swap back to `github-pr-reviewer` once the external repo is corrected. A PR restoring root `SKILL.md`/`TRIGGER.md` (matching this repo's `tests/fixtures/fleetbundle/github-pr-reviewer` fixture) is open at `agentsfleet/github-pr-reviewer#5`. `~/Projects/docs` `quickstart.mdx`, `fleets/install.mdx`, `fleets/library.mdx`, and `fleets/webhooks.mdx` now reference `github-pr-reviewer` again.
- **Merged delivery** — product PR #513, docs PR #137, and the authorized external bundle repair PR #5 are merged. The final closure branch carries only the milestone records and the fleet-library stale-row regression fix.
- **Skill-chain outcomes** — `/write-unit-test`, `/write-integration-test`, and local `/review` complete; `kishore-babysit-prs` runs after the closure PR is pushed.
- **Deferrals** — none.
