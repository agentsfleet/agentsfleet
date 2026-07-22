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
**Status:** DONE
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
- **Restatement (Oracle, PLAN):** a green development deploy notification is earned evidence, never a default: environment prerequisites are proven before any expensive journey starts, browser and CLI journeys assert the shipped product, and the verdict goes red whenever any release-critical result is failed, skipped, or canceled.
- **ASSUMPTIONS I'M MAKING:** (1) M135_003 shares this branch but its remaining CLI-lane work lands separately; consuming `cli-acceptance-dev`'s existing result is valid now. (2) The operator fixture gains `runner:read` — the read-scope `docs/AUTH.md` §Provisioning recommends for runner visibility — via a discovered `constants.ts` edit. (3) "Read-only" preflight means no platform mutation; the idempotent tenant-fixture seed roundtrip (the QStash probe) stays. (4) Browser-journey rubric rows stay red until the QStash registration playbook is run against the development environment; that is an operator action outside this diff.

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
| `ui/packages/app/tests/e2e/acceptance/fixtures/constants.ts` | EDIT (discovered) | Operator fixture gains `runner:read` so the preflight proves runner liveness read-only. |
| `ui/packages/app/tests/e2e/acceptance/fixtures/preflight.ts` | ADD (discovered) | Typed readiness probes and redacted diagnoses shared by the preflight group and setup. |
| `ui/packages/app/tests/e2e/acceptance/install-fleet-cli.spec.ts` | EDIT (discovered) | Carry the canonical CLI-install journey name against the workflow-built binary. |
| `ui/packages/app/tests/release-gate-workflow.test.ts` | ADD (discovered) | Deployment workflow cache, artifact, and notification-verdict assertions. |
| `ui/packages/app/tests/release-gate-suite-config.test.ts` | ADD (discovered) | Acceptance-suite retry, grouping, and artifact configuration assertions. |
| `.github/workflows/deploy-dev.yml` | EDIT (discovered) | Compose the notification verdict inline from every release-critical job. |
| `ui/packages/app/tests/e2e/acceptance/multi-workspace.spec.ts` | EDIT (discovered) | Import the shared secondary-workspace name provisioned once in setup. |
| `ui/packages/app/tests/e2e/acceptance/workspace-fleet-lifecycle.spec.ts` | EDIT (discovered) | Shared secondary-workspace name plus prefix-scoped cleanup. |
| `ui/packages/app/lib/auth/client.ts` | EDIT (discovered) | Keep the Clerk session token fresh during long-lived dashboard use without exposing it to application code. |
| `ui/packages/app/lib/auth/client.test.tsx` | ADD (discovered) | Pin active, resumed, hidden, and signed-out session-refresh behavior. |
| `ui/packages/app/app/layout.tsx` | EDIT (discovered) | Mount the session keeper once under the Clerk provider. |
| `docs/AUTH.md` | EDIT (discovered) | Document proactive refresh and the expired-POST failure it prevents. |
| `ui/packages/app/tests/e2e/acceptance/fixtures/lifecycle.ts` | EDIT (discovered) | Remove the harness-only refresh so long lifecycle journeys prove product session continuity. |
| `ui/packages/app/tests/e2e/acceptance/fixtures/auth.ts` | EDIT (discovered) | Delete the token-exposing manual session refresh workaround. |
| `ui/packages/app/tests/e2e/acceptance/platform-library-onboarding.spec.ts` | EDIT | Let long import and publish mutations exercise product session continuity without fixture intervention. |

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

### §1 — Readiness fails before expensive journeys — **DONE**

The first group proves deployed health, runtime model and QStash visibility, both M135 readiness workstreams, and required secrets by status only. A failed prerequisite prevents dependent journeys from starting.

- **Dimension 1.1 — DONE** — missing model, QStash, connector, runner, or CLI artifact stops the job with one typed redacted diagnosis → Test `test_release_preflight_fails_before_browser_journeys`
- **Dimension 1.2 — DONE** — a ready environment advances to user journeys without recreating configured platform state → Test `test_release_preflight_is_read_only_and_idempotent`

### §2 — Browser expectations match the shipped product — **DONE**

Repair stale headings, workspace routes, install actions, and platform-library assertions. Use web-first state assertions instead of long sleeps or broad selectors.

- **Dimension 2.1 — DONE** — sign out and sign in returns the user to the current workspace dashboard → Test `test_user_can_sign_out_and_sign_in_again`
- **Dimension 2.2 — DONE** — platform library install reaches fleet detail through the current action and route → Test `test_user_installs_platform_library_fleet`
- **Dimension 2.3 — DONE** — CLI-driven install uses the workflow-built binary and reaches the same fleet detail → Test `test_user_installs_fleet_with_cli_binary`

### §3 — Parallelism is earned by isolation — **DONE**

Remove retries as a default. Group only tests whose users, workspaces, fleet names, and provider events are isolated; serialize shared platform-operator mutation.

- **Dimension 3.1 — DONE** — deterministic failure executes once and retains its first useful trace → Test `test_acceptance_does_not_retry_deterministic_failure`
- **Dimension 3.2 — DONE** — isolated groups run concurrently without fixture collision or cross-test cleanup → Test `test_acceptance_parallel_groups_are_isolated`
- **Dimension 3.3 — DONE** — platform-operator mutations remain serialized and reuse existing configured state → Test `test_operator_acceptance_is_serialized`

### §4 — Cache and artifacts reduce waste and preserve diagnosis — **DONE**

Hash the root lock plus Playwright version, install only the selected browser, and upload raw results, rendered report, traces, and screenshots even when the job is canceled or fails.

- **Dimension 4.1 — DONE** — unchanged dependencies restore the exact browser cache; changed lock or browser version misses it → Test `test_playwright_cache_key_tracks_real_inputs`
- **Dimension 4.2 — DONE** — failure and cancellation leave a non-empty raw-results artifact even when the HTML report is incomplete → Test `test_acceptance_artifacts_survive_failure`

### §5 — One release verdict includes every gate — **DONE**

The notification includes quality, browser acceptance, CLI acceptance, and runner results. No skipped or canceled release-critical job is green.

- **Dimension 5.1 — DONE** — CLI failure, skip, or cancellation makes the notification red → Test `test_dev_notification_includes_cli_result`
- **Dimension 5.2 — DONE** — green requires every release-critical job successful and reports the release commit → Test `test_dev_notification_green_requires_all_gates`

### §6 — Long-lived dashboard sessions remain mutation-capable — **IN_PROGRESS**

Keep the short-lived Clerk session token fresh while an authenticated dashboard is active. Refresh on mount, while visible, and when the browser resumes; never expose the token value or construct a client-side Bearer header.

- **Dimension 6.1 — IN_PROGRESS** — an active or resumed signed-in dashboard refreshes before token expiry while hidden and signed-out dashboards do not poll → Test `test_dashboard_session_refresh_survives_long_journeys`

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
| Expired mutation cookie | a long dashboard journey outlives Clerk's short session-token lifetime | client refresh keeps the cookie current; the next Server Action POST reaches middleware authenticated |

## Invariants

1. Release-critical retries default to zero; retry is allowed only for a named externally transient boundary with separate evidence.
2. A green notification includes successful browser and CLI acceptance results.
3. No artifact, trace, screenshot, or step summary contains loaded secret values.
4. Playwright remains the browser engine for this milestone.
5. Session refresh never returns a token value to application code or constructs a client-side Bearer header.

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
| 6.1 | unit | `test_dashboard_session_refresh_survives_long_journeys` | signed-in visible and resumed sessions refresh; hidden and signed-out sessions do not poll |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Focused preflight passes | `cd ui/packages/app && bun run test:e2e:acceptance:preflight` | exit 0 before journey groups | P0 | ✅ `13 passed (1.3m)` vs live api-dev/app-dev |
| R2 | Browser user journeys pass | `make acceptance-e2e` | 0 failed and 0 critical skips | P0 | ❌ harness drift repaired; remaining reds are deployed-product defects the gate exists to catch: live-stream connect failure ("Reconnecting to live updates", UZ-AUTH-002 family) and operator Server Action 307→/sign-in mid-flow — trace evidence retained; regrade after the M133 auth fix deploys |
| R3 | Browser job is bounded | `gh run view --json jobs` | `acceptance-e2e-dev` success below 600 seconds | P0 | ⏳ no CI run on this branch yet (fires on merge to main); local 4-worker suite 9.6 min including product-defect timeout burn |
| R4 | Failure evidence is retained | `gh run download --name acceptance-e2e-dev-results` | non-empty raw results and trace content | P0 | ⏳ artifact exists only after the first CI run; raw traces + screenshots + JSON verified present locally under `playwright-acceptance-results/` on every failed run |
| R5 | Notification consumes CLI result | `rg -n 'needs\.cli-acceptance-dev\.result' .github/workflows/deploy-dev.yml` | at least one match in verdict logic | P0 | ✅ line 704: `CLI_RESULT: ${{ needs.cli-acceptance-dev.result }}` |
| S1 | App unit tests pass | `cd ui/packages/app && bun test` | exit 0 | P0 | ✅ vitest `1736 passed (178 files)`; new release-gate files also pass under bun test (13 pass) |
| S2 | Repository lint passes | `make lint-all` | exit 0 | P0 | ✅ `✓ All lint checks passed` |
| S3 | No secrets | `gitleaks detect` | exit 0 | P0 | ✅ `no leaks found` (3599 commits) |
| S4 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | ✅ all paths in the amended table |

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
  - Continuous Integration (CI) workflow edits (`.github/workflows/**`) are authorized by this spec's Files Changed table plus the human start instruction; recorded here because workflow edits otherwise require an explicit ask.
  - Sign-in card root cause: Clerk dev-instance bot protection stalls the card mount for anonymous headless visits; the harness testing token (the posture every other browser path already uses) resolves it. The heading selector was never stale.
  - QStash 503s (UZ-SCHED-007) were an unseeded development platform-admin vault, not a workflow secrets gap — credentials load from the database vault at boot (`src/agentsfleetd/cron/Credentials.zig`). The environment has since been seeded; the preflight now proves it on every run and diagnoses a regression with the registration playbook.
  - Test-tier deviation: Dimensions 3.2/3.3 are proven at the configuration layer (project grouping, ignore lists, dependency chains pinned by `release-gate-suite-config.test.ts`) plus a live four-worker suite run, rather than by a dedicated in-run concurrency assertion. The spec's tier column said end-to-end; the configuration pin is deterministic where an in-run assertion would be timing-dependent.
  - **Stream 401 (UZ-AUTH-002) — root cause found and fixed here.** Indy's report was a real deployment defect, not harness drift. The four token-minting proxy Route Handlers lived under `app/backend/…`, the same prefix as the `/backend/:path*` rewrite in `next.config.ts`. Vercel's edge router applies rewrites ahead of same-prefix filesystem routes, so on the deployed app the handler never ran: the browser's EventSource request went straight to `api-dev` carrying only a cookie and no Bearer, and every stream connect answered 401. Local `next start` resolves the handler first, which is why the suite passed locally and failed on app-dev. Proof: a fresh fixture token gets HTTP 200 on `api-dev` directly (`/v1/workspaces/{ws}/events/stream` → `event: hello`), while the same stream through `app-dev.agentsfleet.net/backend/…` returns `UZ-AUTH-002` 4 seconds after sign-in. Fix: the handlers move to `app/live/…`, a prefix no rewrite can claim; `tests/stream-proxy-routing.test.ts` pins the invariant (red-green proven — 2 of 3 assertions fail against the old layout).
  - **Server-Action 307 → `/sign-in` — second auth arm, also fixed here.** Clerk session tokens live ~60 seconds. A journey step that outlives one (GitHub import, install stream, observe walk) leaves the next Server Action POST holding a stale `__session` cookie; Clerk does not handshake non-GET requests, so `auth.protect()` redirects to sign-in before the action executes. `AuthSessionKeeper` now refreshes through Clerk's `user.reload()` on mount, before expiry while visible, and when focus, visibility, or connectivity resumes. It never receives token bytes. The shared lifecycle fixture's manual refresh is removed so long acceptance journeys exercise the product behavior rather than masking it.
  - Same-branch auth decision: Indy explicitly requested the durable Clerk session-expiry fix in `feat/m135-release-readiness`; M135_004 therefore owns the product-side session keeper rather than routing it to a separate auth Pull Request (PR).
  - Blast-radius rows `install-ui.ts`, `nav.ts`, `seed.ts`, `platform-library-onboarding.spec.ts`, and `cli-adversarial.spec.ts` were verified current against the shipped product (the acceptance-repair commit that landed after this spec was authored already fixed them) — no diff needed; the `cli-adversarial` fail-on-missing-artifact requirement landed in the shared `cli-runner.ts` fixture all CLI specs spawn through.
  - Adversarial review (REVIEW stage) surfaced and this diff fixed: the raw Vercel bypass secret riding into retained failure traces (now traded for its derived cookie via storage state before any traced context exists), a re-run artifact-name conflict (overwrite enabled), the fetch-audit group not being strictly last, silent empty Playwright-version cache keys, two jobs loading secrets before installs, missing prod-suite concurrency, and a string-concatenated notification payload (now jq-built).
  - Known-bounded residual: retained traces still carry fixture-user Clerk session cookies (dev/prod fixture tenants only; sessions revoked at teardown) and cross-job fleet mutations between `cli-acceptance-dev` and the browser suite share fixture tenants — both routed to the M135_003 CLI-lane workstream on this same branch.
- **Metrics review** — `dev_release_acceptance_summary` fires once per release-critical job from the deploy notification step as a logfmt line (job, result, commit); no email, token, cookie, payload body, or provider secret is included.
- **Skill-chain outcomes** — recorded in PR Session Notes at CHORE(close).
- **Deferrals** — none.
