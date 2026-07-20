<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the kishore-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M135_004: Development User Interface (UI) release acceptance fails fast and tells the truth

**Prototype:** v2.0.0
**Milestone:** M135
**Workstream:** 004
**Date:** Jul 20, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — the browser job reaches its workflow cap on deterministic drift, and the deployment notification can still say release-ready without the CLI result.
**Categories:** INFRA, UI
**Batch:** B3 — integrates the prerequisite and CLI workstreams into one release gate
**Branch:** `feat/m135-release-readiness`
**Test Baseline:** unit=2802 integration=369
**Depends on:** M135_001 (connectors proven), M135_002 (runner online), M135_003 (CLI lane exports truthful result)
**Provenance:** human-directed, Oracle-authored from Actions runs 29680816280, 29708730980, current Playwright configuration, and browser acceptance sources
**Canonical architecture:** `docs/architecture/scenarios/github-pr-reviewer.md` §What John sees; `docs/VERIFY_TIERS.md` §Correctness tiers

---

## Overview

**Goal (testable):** `acceptance-e2e-dev` validates prerequisites once, executes current browser journeys with isolated fixtures and useful artifacts, completes within a 600-second job budget, and the development notification is green only when browser, CLI, quality, and runner gates all pass.
**Problem:** The latest End-to-End (E2E) browser job ran 53 tests with one worker and two retries until cancellation. Known deterministic failures include stale headings and routes, an unbuilt CLI artifact, old install selectors, and a platform-library wait that consumes repeated long timeouts. The browser cache hashes a nonexistent package lock, cancellation can lose raw results, and the notification ignores `cli-acceptance-dev` despite listing it as a dependency.
**Solution summary:** Add a release-readiness preflight, repair expectations to match shipped user behavior, build the CLI once before browser journeys that invoke it, remove blanket retries, parallelize only fixture-isolated groups, cache against the real lock and browser version, upload raw plus rendered results, and include every gate in the notification. Keep Playwright: replacing it with the alpha Rustwright rewrite would add migration risk while leaving application waits, fixture drift, and backend failures untouched.

## PR Intent & comprehension handshake

- **PR title (eventual):** ci(dev): make release acceptance fast and truthful
- **Intent (one sentence):** A green development deploy notification is credible evidence that a user can sign in, install and operate fleets, and use the CLI against a provider-ready environment.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `.github/workflows/deploy-dev.yml` — current browser, CLI, artifact, cache, timeout, and notification behavior.
2. `ui/packages/app/playwright.acceptance.config.ts` — retry, worker, output, and server-lifecycle settings.
3. `ui/packages/app/tests/e2e/acceptance/_smoke.spec.ts` and `global-setup.ts` — earliest safe preflight and suite fixture ownership.
4. `ui/packages/app/tests/e2e/acceptance/fixtures/seed.ts`, `install-ui.ts`, `nav.ts`, and `cli-runner.ts` — shared high-cost setup and current product selectors.
5. GitHub Actions run `29708730980` jobs `88261664820` and `88261664829` — current failure evidence after PR 534.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `.github/workflows/deploy-dev.yml` | EDIT | Wire readiness, correct cache/build/artifacts, consume CLI result, and enforce truthful notification. |
| `.github/workflows/smoke-post-deploy.yml` | EDIT | Keep production browser runner and artifact posture aligned where applicable. |
| `ui/packages/app/playwright.acceptance.config.ts` | EDIT | Remove blanket retries and enable only safe, isolated concurrency. |
| `ui/packages/app/package.json` | EDIT | Add focused preflight and suite-group commands without a new make target. |
| `ui/packages/app/tests/e2e/acceptance/_smoke.spec.ts` | EDIT | Fail fast on deployed app, model, QStash, connector, and runner readiness. |
| `ui/packages/app/tests/e2e/acceptance/global-setup.ts` | EDIT | Build or validate shared prerequisites once and retain redacted diagnostics. |
| `ui/packages/app/tests/e2e/acceptance/fixtures/cli-runner.ts` | EDIT | Use the built CLI artifact and fail immediately when absent. |
| `ui/packages/app/tests/e2e/acceptance/fixtures/install-ui.ts` | EDIT | Follow current install controls and stable accessible names. |
| `ui/packages/app/tests/e2e/acceptance/fixtures/nav.ts` | EDIT | Follow current workspace-scoped routes. |
| `ui/packages/app/tests/e2e/acceptance/fixtures/seed.ts` | EDIT | Keep fixture resources isolated and reusable within one worker. |
| `ui/packages/app/tests/e2e/acceptance/signout-and-signin.spec.ts` | EDIT | Assert the current sign-in surface. |
| `ui/packages/app/tests/e2e/acceptance/login-install-lifecycle.spec.ts` | EDIT | Prove current install success and lifecycle without stale waits. |
| `ui/packages/app/tests/e2e/acceptance/workspace-url-flow.spec.ts` | EDIT | Assert canonical workspace-scoped navigation. |
| `ui/packages/app/tests/e2e/acceptance/platform-library-onboarding.spec.ts` | EDIT | Use current platform-library controls and bounded state assertions. |
| `ui/packages/app/tests/e2e/acceptance/cli-adversarial.spec.ts` | EDIT | Consume the one workflow-built CLI and fail on missing artifact. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — Test Naming (TST-NAM), Unified Form for Symbols (UFS), No Dead Code (NDC), No Legacy Retained (NLR), Orphan Sweep (ORP), Prompt-injection Resistance (PRI): behavior-named tests, shared constants, no stale retry branch, and redacted artifacts.
- **`dispatch/write_ts_adhere_bun.md`** — Bun-native helpers, typed boundaries, constants, focused files, and cleanup.
- **`dispatch/write_shell.md`** — workflow shell fragments quote variables and never echo loaded secrets.
- **`docs/AUTH.md`** — browser sessions, operator scopes, and fixture identities remain real.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| TypeScript (TS) FILE SHAPE | no new production TypeScript file | existing test modules retain their current roles |
| File & Function Length | yes | split shared setup by responsibility rather than growing global setup |
| UFS | yes | named selectors, timeouts, artifact paths, and job result labels |
| UI Substitution / DESIGN TOKEN | no | no product component change |
| LOGGING / LIFECYCLE | yes | redact fixtures, close contexts, revoke sessions, upload evidence on failure |

## Prior-Art / Reference Implementations

- **Reference:** `.github/workflows/smoke-post-deploy.yml` — production acceptance structure and always-upload evidence pattern.
- **Reference:** Playwright's checked-in acceptance fixtures — one browser context per owned identity and accessible-name selectors.

## Sections (implementation slices)

### §1 — Readiness fails before expensive journeys

The first group proves deployed health, runtime model and QStash visibility, both M135 readiness workstreams, and required secrets by status only. A failed prerequisite prevents dependent journeys from starting.

- **Dimension 1.1** — missing model, QStash, connector, runner, or CLI artifact stops the job with one typed redacted diagnosis → Test `test_release_preflight_fails_before_browser_journeys`
- **Dimension 1.2** — a ready environment advances to user journeys without recreating configured platform state → Test `test_release_preflight_is_read_only_and_idempotent`

### §2 — Browser expectations match the shipped product

Repair stale headings, workspace routes, install actions, and platform-library assertions. Use web-first state assertions instead of long sleeps or broad selectors.

- **Dimension 2.1** — sign out and sign in returns the user to the current workspace dashboard → Test `test_user_can_sign_out_and_sign_in_again`
- **Dimension 2.2** — platform library install reaches fleet detail through the current action and route → Test `test_user_installs_platform_library_fleet`
- **Dimension 2.3** — CLI-driven install uses the workflow-built binary and reaches the same fleet detail → Test `test_user_installs_fleet_with_cli_binary`

### §3 — Parallelism is earned by isolation

Remove retries as a default. Group only tests whose users, workspaces, fleet names, and provider events are isolated; serialize shared platform-operator mutation.

- **Dimension 3.1** — deterministic failure executes once and retains its first useful trace → Test `test_acceptance_does_not_retry_deterministic_failure`
- **Dimension 3.2** — isolated groups run concurrently without fixture collision or cross-test cleanup → Test `test_acceptance_parallel_groups_are_isolated`
- **Dimension 3.3** — platform-operator mutations remain serialized and reuse existing configured state → Test `test_operator_acceptance_is_serialized`

### §4 — Cache and artifacts reduce waste and preserve diagnosis

Hash the root lock plus Playwright version, install only the selected browser, and upload raw results, rendered report, traces, and screenshots even when the job is canceled or fails.

- **Dimension 4.1** — unchanged dependencies restore the exact browser cache; changed lock or browser version misses it → Test `test_playwright_cache_key_tracks_real_inputs`
- **Dimension 4.2** — failure and cancellation leave a non-empty raw-results artifact even when the HTML report is incomplete → Test `test_acceptance_artifacts_survive_failure`

### §5 — One release verdict includes every gate

The notification includes quality, browser acceptance, CLI acceptance, and runner results. No skipped or canceled release-critical job is green.

- **Dimension 5.1** — CLI failure, skip, or cancellation makes the notification red → Test `test_dev_notification_includes_cli_result`
- **Dimension 5.2** — green requires every release-critical job successful and reports the release commit → Test `test_dev_notification_green_requires_all_gates`

## Interfaces

```text
acceptance-e2e-dev preflight -> browser journey groups -> artifacts
cli-acceptance-dev           -> result consumed by notify
notify green                 = qa-dev success
                             + acceptance-e2e-dev success
                             + cli-acceptance-dev success
                             + deploy-worker-dev success or documented skip
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Runtime prerequisite missing | deployment does not see model, QStash, connector, or online runner | preflight fails; zero dependent browser tests start |
| Product drift | heading, route, or action changed | focused web-first assertion fails once with trace and screenshot |
| Missing CLI build | browser scenario invokes absent `dist` binary | build step or preflight fails before browser navigation |
| Fixture collision | concurrent groups share user or resource identity | isolation assertion fails; colliding group returns to serialized mode |
| Cache drift | lock or browser version changed | exact cache miss and fresh selected-browser install |
| Cancellation | workflow cap or external interruption | raw results and traces upload under `always()` |
| False green | CLI or browser result skipped, canceled, or failed | notify emits red verdict with all job results |

## Invariants

1. Release-critical retries default to zero; retry is allowed only for a named externally transient boundary with separate evidence.
2. A green notification includes successful browser and CLI acceptance results.
3. No artifact, trace, screenshot, or step summary contains loaded secret values.
4. Playwright remains the browser engine for this milestone.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `dev_release_acceptance_summary` | ops | each release-critical job ends | job, group, registered, passed, failed, skipped, duration, result | no email, token, cookie, payload body, or provider secret | `test_dev_notification_green_requires_all_gates` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_release_preflight_fails_before_browser_journeys` | each absent prerequisite stops before dependent test registration |
| 1.2 | integration | `test_release_preflight_is_read_only_and_idempotent` | ready preflight makes no platform mutation |
| 2.1 | end-to-end | `test_user_can_sign_out_and_sign_in_again` | current sign-in returns to workspace dashboard |
| 2.2 | end-to-end | `test_user_installs_platform_library_fleet` | current UI action reaches installed fleet detail |
| 2.3 | end-to-end | `test_user_installs_fleet_with_cli_binary` | built CLI installs and browser observes same fleet |
| 3.1 | integration | `test_acceptance_does_not_retry_deterministic_failure` | injected selector failure has one attempt |
| 3.2 | end-to-end | `test_acceptance_parallel_groups_are_isolated` | concurrent resources and cleanup remain disjoint |
| 3.3 | end-to-end | `test_operator_acceptance_is_serialized` | platform mutations never overlap |
| 4.1 | integration | `test_playwright_cache_key_tracks_real_inputs` | key changes with root lock or browser version |
| 4.2 | integration | `test_acceptance_artifacts_survive_failure` | failed run retains raw results and traces |
| 5.1 | integration | `test_dev_notification_includes_cli_result` | CLI non-success yields red notification |
| 5.2 | integration | `test_dev_notification_green_requires_all_gates` | only all-success matrix yields green |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Focused preflight passes | `cd ui/packages/app && bun run test:e2e:acceptance:preflight` | exit 0 before journey groups | P0 | |
| R2 | Browser user journeys pass | `make acceptance-e2e` | 0 failed and 0 critical skips | P0 | |
| R3 | Browser job is bounded | `gh run view --json jobs` | `acceptance-e2e-dev` success below 600 seconds | P0 | |
| R4 | Failure evidence is retained | `gh run download --name acceptance-e2e-dev-results` | non-empty raw results and trace content | P0 | |
| R5 | Notification consumes CLI result | `rg -n 'needs\.cli-acceptance-dev\.result' .github/workflows/deploy-dev.yml` | at least one match in verdict logic | P0 | |
| S1 | App unit tests pass | `cd ui/packages/app && bun test` | exit 0 | P0 | |
| S2 | Repository lint passes | `make lint-all` | exit 0 | P0 | |
| S3 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S4 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

N/A — no files deleted. Removed retry and stale-selector branches require zero-reference greps in PR Session Notes.

## Out of Scope

- Replacing Playwright with Rustwright while Rustwright remains alpha and Chromium-only.
- Expanding browser acceptance into visual-regression coverage.
- Configuring production provider connections or minting production runner tokens.

## Product Clarity (authoring record)

1. **Successful user moment** — a release operator sees one green verdict after browser and CLI users both complete real journeys.
2. **Preserved user behaviour** — sign-up, sign-in, workspaces, libraries, fleets, secrets, models, billing, approvals, logs, events, and deletion remain covered.
3. **Optimal-way check** — repair readiness and test truth before changing browser engines; the present bottleneck is application state and harness drift.
4. **Rebuild-vs-iterate** — refactor suite grouping and workflow evidence while retaining test intent.
5. **What we build** — preflight, current selectors, isolated grouping, exact cache, durable artifacts, all-gate notification.
6. **What we do NOT build** — browser-engine migration, accepted flaky retries, or a green verdict with skips.
7. **Fit with existing features** — composes connector, runner, platform-library, browser, CLI, and deployment proof.
8. **Surface order** — both: browser proves dashboard journeys; CLI proves automation and terminal workflows.
9. **Dashboard restraint** — no product UI changes solely to satisfy tests; tests follow accessible shipped behavior.
10. **Confused-user next step** — the first failing group and retained trace identify readiness, auth, install, lifecycle, or workflow verdict.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** the browser harness and deployment verdict share one workstream because cache, artifacts, grouping, and notification are one release signal.
- **Alternatives considered:** a Rustwright rewrite was rejected because it does not repair backend readiness, stale selectors, fixture ownership, missing builds, or false-green logic.
- **Patch-vs-refactor verdict:** this is a **refactor** because execution topology, evidence, and verdict semantics change together while user coverage stays stable.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage:
- **Metrics review** —
- **Skill-chain outcomes** —
- **Deferrals** —
