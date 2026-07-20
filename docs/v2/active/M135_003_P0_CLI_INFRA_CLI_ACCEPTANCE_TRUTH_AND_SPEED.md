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

# M135_003: Command-Line Interface (CLI) acceptance is fast and proves a real user result

**Prototype:** v2.0.0
**Milestone:** M135
**Workstream:** 003
**Date:** Jul 20, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — the current CLI job either fails on missing fixture identities or spends most of its run accepting inconclusive steer timeouts.
**Categories:** CLI, INFRA
**Batch:** B2 — starts only after M135_001 and M135_002 pass
**Branch:** `feat/m135-release-readiness`
**Test Baseline:** unit=2802 integration=369
**Depends on:** M135_001 (provider-ready fleet path), M135_002 (online runner executes live steer)
**Provenance:** human-directed, Oracle-authored from Actions runs 29680816280 and 29708730980 plus current CLI fixtures
**Canonical architecture:** `docs/architecture/data_flow.md` §Running one event; `docs/AUTH.md` §CLI browser handoff

---

## Overview

**Goal (testable):** `cli-acceptance-dev` creates or reuses owned fixture users safely, exercises the actual browser login handoff, requires a live fleet steer to reach a terminal success, and completes within a 600-second job budget with zero release-critical skips.
**Problem:** The latest run registered 87 tests but failed 16 because fixture users were absent and no creation password was supplied. When those users existed, 206 tests passed in 600.84 seconds, with three steering cases consuming 365.68 seconds while treating timeout-like outcomes as acceptable. Browser dependencies are installed even when the login handshake is disabled.
**Solution summary:** Make fixture ownership self-healing without stable shared passwords, run the browser handshake deliberately, separate deterministic command coverage from one golden live execution, require Server-Sent Events (SSE) or fallback polling to observe an actual terminal result, and remove duplicate live waits. Do not shorten production timeouts merely to make tests green.

## PR Intent & comprehension handshake

- **PR title (eventual):** test(cli): make development acceptance truthful and bounded
- **Intent (one sentence):** A green CLI acceptance job proves that a new user can authenticate, manage resources, and receive one real fleet result without waiting through redundant inconclusive timeouts.
- **Handshake** — the implementing agent fills this at PLAN, before EXECUTE: restate the Intent in its own words and list `ASSUMPTIONS I'M MAKING: …`. A mismatch between the restatement and the Intent above → STOP and reconcile before any edit.

## Implementing agent — read these first

1. `cli/test/acceptance/fixtures/clerk-admin.ts` — current missing-user failure and session mint path.
2. `ui/packages/app/tests/e2e/acceptance/fixtures/clerk-admin.ts` — owned, idempotent fixture-user pattern to align with without cross-suite ordering.
3. `cli/test/acceptance/lifecycle-after-login.spec.ts` and `login-negatives.spec.ts` — browser handoff opt-in and current skip condition.
4. `cli/test/acceptance/steer-live.spec.ts` and `streaming-follow.spec.ts` — live-result duplication and weak success assertions.
5. `cli/src/commands/fleet_steer_events.ts` — production SSE fallback semantics that tests must prove, not bypass.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `cli/test/acceptance/fixtures/clerk-admin.ts` | EDIT | Create missing owned users with ephemeral strong credentials while preserving direct session minting. |
| `cli/test/acceptance/fixtures/clerk-admin.test.ts` | CREATE | Failure-inject user lookup, creation, reuse, and redaction behavior. |
| `cli/test/acceptance/global-setup.ts` | EDIT | Establish suite-owned identities and one isolated golden live fixture without browser-suite ordering. |
| `cli/test/acceptance/lifecycle-after-login.spec.ts` | EDIT | Make the real browser handoff a release-critical scenario. |
| `cli/test/acceptance/login-negatives.spec.ts` | EDIT | Keep negative login cases deterministic while sharing browser setup policy. |
| `cli/test/acceptance/steer-live.spec.ts` | EDIT | Require one real terminal result and remove redundant live turns. |
| `cli/test/acceptance/streaming-follow.spec.ts` | EDIT | Pin SSE terminal delivery and fallback behavior without accepting timeout as success. |
| `cli/test/acceptance/fixtures/streaming-ops.ts` | EDIT | Centralize golden live result assertions and bounded evidence capture. |
| `cli/package.json` | EDIT | Split deterministic and live acceptance commands while retaining one aggregate entrypoint. |
| `cli/src/commands/fleet_steer_events.ts` | EDIT (conditional) | Change only if the golden live proof exposes a production SSE/fallback defect; never lower the timeout for test speed. |

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — Test Naming (TST-NAM), Unified Form for Symbols (UFS), Tagged Union (TGU), No Dead Code (NDC), No Legacy Retained (NLR), Orphan Sweep (ORP), Prompt-injection Resistance (PRI): behavior names, constants, tagged outcomes, no dormant fixture modes, and no secret output.
- **`dispatch/write_ts_adhere_bun.md`** — Bun-native tests, parse-boundary narrowing, constant discipline, and focused test files.
- **`docs/AUTH.md`** — CLI browser handoff and session-token boundaries remain real.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| TypeScript (TS) FILE SHAPE | yes for the new fixture test | test-only module; no production state abstraction added |
| File & Function Length | yes | split fixture transport from lifecycle if caps approach |
| UFS | yes | named timeout, outcome, fixture-owner, and mode constants |
| LOGGING / LIFECYCLE | yes | revoke sessions, clean state directories, and redact identity credentials |

## Prior-Art / Reference Implementations

- **Reference:** `ui/packages/app/tests/e2e/acceptance/fixtures/clerk-admin.ts` — suite-owned user creation, metadata, bootstrap, and session revocation.
- **Reference:** Supabase-style CLI test pyramid — pure command matrix, subprocess integration, then a small live operational lane.

## Sections (implementation slices)

### §1 — Fixture identities self-heal securely

Missing users are created with generated strong credentials and suite ownership metadata. Direct token minting does not depend on knowing the password; real browser login uses the existing secure handoff path instead of a shared static password.

- **Dimension 1.1** — missing regular and admin users are created, bootstrapped, and reusable without a vault-stored password → Test `test_cli_fixture_creates_missing_owned_user`
- **Dimension 1.2** — an existing non-fixture identity at the configured email is refused rather than adopted → Test `test_cli_fixture_refuses_unowned_user`
- **Dimension 1.3** — setup and teardown redact credentials and revoke minted sessions → Test `test_cli_fixture_teardown_revokes_and_redacts`

### §2 — Browser login is a real release scenario

The aggregate acceptance command enables the login handshake explicitly. Browser installation is conditional on this lane, and a disabled handshake is a failure in the release job rather than a misleading skip.

- **Dimension 2.1** — browser handoff signs in, approves the session, and leaves the CLI authenticated → Test `test_cli_login_browser_handoff_completes`
- **Dimension 2.2** — missing browser prerequisites fail preflight before the rest of the suite → Test `test_cli_login_preflight_fails_loud`

### §3 — One golden live steer must finish

Reuse one installed fleet and one live steer across the operational assertions. Green requires a processed terminal event or explicit fleet terminal failure; timeout, cancel, and indeterminate outcomes fail.

- **Dimension 3.1** — live steer receives a terminal result through SSE and prints it in human and JSON modes → Test `test_cli_live_steer_returns_terminal_result`
- **Dimension 3.2** — forced stream loss exercises bounded fallback polling and still reaches the same terminal result → Test `test_cli_live_steer_fallback_returns_terminal_result`
- **Dimension 3.3** — an online runner or provider failure is classified distinctly from a transport timeout → Test `test_cli_live_steer_failure_is_actionable`

### §4 — Deterministic breadth stays broad; remote work stays small

The command, error, lifecycle, and concurrency matrix remains broad, but only the golden scenario pays for external execution. Report registered, passed, failed, and skipped counts by lane.

- **Dimension 4.1** — deterministic cases do not enqueue remote fleet executions → Test `test_cli_deterministic_lane_has_no_live_steer`
- **Dimension 4.2** — release summary reports zero critical skips and job duration within budget → Test `test_cli_acceptance_summary_is_release_grade`

## Interfaces

```text
bun run test:acceptance:deterministic   broad CLI behavior, no live fleet work
bun run test:acceptance:live            browser handoff plus one golden execution
bun run test:acceptance                 build, deterministic lane, then live lane

Live success = processed terminal event or explicit fleet terminal failure.
Timeout, cancellation, and indeterminate outcomes are not success.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Missing fixture user | development database or Clerk state was reset | create owned user, bootstrap tenant, continue idempotently |
| Unowned email collision | configured email belongs to a non-fixture identity | fail before mutation and name the ownership mismatch |
| Browser disabled | handshake opt-in or key missing | fail preflight in release lane; deterministic lane remains runnable |
| Runner unavailable | no online runner can lease the event | fail with runner-readiness diagnosis, not a generic timeout |
| SSE interruption | live stream closes before terminal event | use bounded polling fallback and require the same terminal result |
| External stall | neither stream nor polling reaches terminal state | fail and retain redacted event identifiers for diagnosis |

## Invariants

1. No stable fixture password is stored in repository, workflow, or vault.
2. A green live steer always contains a terminal event; timeout is never an accepted outcome.
3. Deterministic cases cannot enqueue remote work; only the named live lane may do so.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| `cli_acceptance_lane_summary` | ops | a deterministic or live lane ends | lane, registered, passed, failed, skipped, duration, terminal outcome | no email, token, command secret, or response body | `test_cli_acceptance_summary_is_release_grade` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | integration | `test_cli_fixture_creates_missing_owned_user` | absent owned email creates, bootstraps, and reuses one user |
| 1.2 | integration | `test_cli_fixture_refuses_unowned_user` | ownership metadata mismatch causes no mutation |
| 1.3 | integration | `test_cli_fixture_teardown_revokes_and_redacts` | teardown revokes sessions and captured output omits credentials |
| 2.1 | end-to-end | `test_cli_login_browser_handoff_completes` | browser approval leaves CLI authenticated |
| 2.2 | integration | `test_cli_login_preflight_fails_loud` | missing browser input aborts before unrelated cases |
| 3.1 | end-to-end | `test_cli_live_steer_returns_terminal_result` | live SSE result reaches human and JSON output |
| 3.2 | end-to-end | `test_cli_live_steer_fallback_returns_terminal_result` | injected stream loss reaches same result through polling |
| 3.3 | integration | `test_cli_live_steer_failure_is_actionable` | runner, provider, and transport failures remain distinct |
| 4.1 | integration | `test_cli_deterministic_lane_has_no_live_steer` | deterministic command run produces zero remote execution calls |
| 4.2 | integration | `test_cli_acceptance_summary_is_release_grade` | summary has zero critical skips and bounded duration |

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | Fixture provisioning self-heals | `cd cli && bun test test/acceptance/fixtures/clerk-admin.test.ts` | exit 0 | P0 | |
| R2 | Deterministic CLI breadth passes | `cd cli && bun run test:acceptance:deterministic` | 0 failed and 0 remote steer calls | P0 | |
| R3 | Browser login and live result pass | `cd cli && bun run test:acceptance:live` | terminal result observed and 0 critical skips | P0 | |
| R4 | Aggregate lane is bounded | `make cli-acceptance` | exit 0 and duration below 600 seconds | P0 | |
| S1 | CLI unit tests pass | `make test-unit-agentsfleet` | exit 0 | P0 | |
| S2 | Repository lint passes | `make lint-all` | exit 0 | P0 | |
| S3 | No secrets | `gitleaks detect` | exit 0 | P0 | |
| S4 | Diff stays inside Files Changed | `git diff --name-only origin/main` | 0 paths missing from the Files Changed table | P0 | |

**Grading protocol (VERIFY):** run the Verify command verbatim; grade ONLY from its output. Graded = ✅/❌ + the one decisive output line; long evidence goes to PR Session Notes with a pointer here. **Ship gate:** every row graded, every P0 ✅ → eligible for CHORE(close); any ❌ or empty cell → return to EXECUTE.

## Dead Code Sweep

N/A — no files deleted. Removed skip branches and duplicate live cases require `rg` proof in PR Session Notes.

## Out of Scope

- Replacing Bun or Playwright.
- Lowering production SSE or polling timeouts solely for test duration.
- Turning every CLI matrix case into a live external execution.

## Product Clarity (authoring record)

1. **Successful user moment** — a fresh user logs in from the CLI, steers a fleet, and sees its terminal answer.
2. **Preserved user behaviour** — help, flags, errors, workspace, secret, fleet, event, and lifecycle coverage stays intact.
3. **Optimal-way check** — one golden live execution proves the remote path without paying that cost in every matrix case.
4. **Rebuild-vs-iterate** — reorganize the harness, not the CLI architecture.
5. **What we build** — secure self-healing fixtures, explicit browser lane, truthful live assertion, lane summaries.
6. **What we do NOT build** — shared passwords, accepted timeouts, or broad live duplication.
7. **Fit with existing features** — proves login, tokens, workspaces, fleets, secrets, logs, events, streaming, and deletion.
8. **Surface order** — CLI-first; browser exists only to complete the CLI login handoff.
9. **Dashboard restraint** — no dashboard feature change; the test uses the shipped approval surface.
10. **Confused-user next step** — failure identifies fixture ownership, browser preflight, runner readiness, provider readiness, or stream transport.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** split deterministic breadth from one live journey while preserving one aggregate command.
- **Alternatives considered:** lowering all timeouts was rejected because it would make failures faster without making success truthful.
- **Patch-vs-refactor verdict:** this is a **refactor** because fixture ownership, lane boundaries, and success semantics must change together.

## Discovery (consult log)

- **Consults** — Architecture / Legacy-Design / gate-flag triage:
- **Metrics review** —
- **Skill-chain outcomes** —
- **Deferrals** —
