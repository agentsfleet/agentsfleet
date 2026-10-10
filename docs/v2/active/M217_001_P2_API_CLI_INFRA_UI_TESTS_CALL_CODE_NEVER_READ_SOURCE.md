<!--
SPEC AUTHORING RULES (load-bearing — the one comment that survives):
- Body order = the executing agent's read order. Fill via the orly-spec-new
  skill (authoring order lives there); after filling, DELETE every "tpl:"
  guidance comment — the SPEC TEMPLATE GATE blocks tpl residue, unfilled
  {slots}, and missing required sections (.orly/audits/spec-template.sh --staged).
- No time/effort/hour/day estimates anywhere. No effort columns, complexity
  ratings, percentage-complete, implementation dates, assigned owners.
- Priority (P0/P1/P2/P3) is the only sizing signal; Dependencies are the only
  sequencing signal. A section that contradicts these rules loses — delete it.
-->

# M217_001: Tests call the code they prove and never read its source

**Prototype:** v2.0.0
**Milestone:** M217
**Workstream:** 001
**Date:** Oct 08, 2026
**Status:** IN_PROGRESS
**Priority:** P2 — test-suite tooling; no product behaviour changes
**Categories:** API (Rust daemon and runner tests), command-line interface (CLI), Infrastructure (INFRA: playbooks, deploy, scripts, make), user interface (UI: app, design-system, website)
**Batch:** B1 — starts once M213 merges; Sections run in order
**Branch:** feat/m217-tests-call-code
**Baseline revision:** c07f5e899ab4b0efa1d5c6a81fcff5eb6bc64741
**Test Baseline:** pending — measured before the Pull Request
**Baseline evidence:** pending — report path or run URL with revision, commands, passed/failed/skipped counts, and environment
**Depends on:** M213_001 (Rust runner cutover, Zig retired; one Pull Request (PR) with M213_002). It removes or rewrites ledger rows F386–F391, renames four ledger tests, and adds the files behind F044 and F124. Re-locate every ledger row by test name after it lands.
**Provenance:** LLM-drafted (large language model Claude Opus 5.5, Oct 08, 2026) from the repository-wide test audit and Indy's Oct 08 decisions
**Canonical architecture:** `docs/architecture/testing.md` §Public lanes, §Coverage

## Overview

**Goal (testable):** `check_tests_call_code` reports zero test files that read repository files; every ledger row F001–F385 is executed, linted, trimmed or deleted; `make test-unit-all` passes with every coverage threshold unchanged.
**Problem:** 403 confirmed findings across 333 files on `main` (`2a21591b3`) pass without proving behaviour. They grep source text, pin copy and class names, repeat stronger tests, never run, or assert on their own mocks. A reflowed file or reworded comment turns them red with no bug; a real regression written another way passes them. Nine can never fail or never run, so the suite reports guarantees nobody holds.
**Solution summary:** Work the ledger (`docs/v2/reviews/m217-test-audit-findings-2026-10-08.md`) in Indy's order. Rewrite the tests whose guarantee is real so they call the code; move text rules over source into repository-owned lints; delete the rest with the production code only they kept alive. Coverage floors stay where they are. One new lint fails `make lint-all` when a test reads a repository file, once Indy approves its wiring. `docs/architecture/testing.md` states the rule.

## PR Intent & comprehension handshake

- **PR title (eventual):** test: make tests call code and never read source
- **Intent (one sentence):** A green suite means the code works, and reflowing a file or rewording a comment never turns it red.
- **Handshake** — Restated at PLAN (Oct 10, 2026): a test goes red only when the behaviour it names breaks, so each test runs the code; rules about how source text reads move to lints that `make lint-all` runs. `ASSUMPTIONS I'M MAKING:` (1) a ledger row whose test is already gone is recorded with the commit that removed it, not re-created; (2) a split row resolves as §2 says (execute beats lint; delete beats lint when the twin exists); (3) a live rewrite whose subject cannot be broken without a schema change counts as red-run when breaking its predicate in the statement turns it red; (4) e2e specs that need a deployed stack are type-checked and linted locally and proven by their workflow run; (5) work runs on one branch per surface, cherry-picked onto `feat/m217-tests-call-code` in Section order. Matches the Intent.

## Implementing agent — read these first

1. `docs/v2/reviews/m217-test-audit-findings-2026-10-08.md` — every site, its action, why it proves nothing, and what breaks if it goes. Rows are the unit of work.
2. `docs/architecture/testing.md` — lanes, the 100% TypeScript and 97.5% Rust floors, isolation rules ISO-1 to ISO-3 for live tests; "the answer is a test, never absorbed slack".
3. `rustd/crates/afd_api/src/openapi.rs` — `document()` builds the OpenAPI document in-process (`openapi_codes.rs:98` reads it); rewrites never open `public/openapi.json`.
4. `scripts/check_documentation_rules.py` — the repository-owned lint shape (standard-library Python plus its `_test.py`) that moved checks follow.
5. `.orly/orly.json` — `managed` lists every `.orly/audits/*` file; `orly update` rewrites them, so no moved check lands there.

## Files Changed (blast radius)

The ledger enumerates each test file by row; this table names the roles.

| File | Action | Why |
|------|--------|-----|
| `docs/v2/pending/M217_001_P2_API_CLI_INFRA_UI_TESTS_CALL_CODE_NEVER_READ_SOURCE.md` → `active/` → `done/` | MOVE / EDIT | Lifecycle, Dimension status, Discovery |
| `docs/v2/reviews/m217-test-audit-findings-2026-10-08.md` | EDIT | Re-located lines after M213; rows already gone are marked with the commit that removed them |
| `rustd/crates/*/tests/**`, `rustd/crates/*/src/**/tests.rs`, inline `#[cfg(test)]` modules in ledger rows | EDIT / DELETE / CREATE | Rust rows; live rewrites land as `integration_*` files in the owning crate |
| `rustd/crates/afd_dragonfly/tests/datastore_suite.rs`, `rustd/crates/afd_dragonfly/src/test_util.rs`, `rustd/crates/afr_supervisor/src/test_support/sandbox.rs`, `rustd/crates/afr_tools/src/sandbox/scripted.rs`, `rustd/crates/afr_agent/src/engine.rs` | EDIT | Module declarations of deleted test files; the diagnosis-only client builder; the moved redaction test (F345) |
| `cli/test/**` | EDIT / DELETE | CLI rows |
| `cli/src/lib/cli-flow.ts`, `cli/src/constants/cli-credential.ts`, `cli/src/services/workspaces.ts`, `cli/src/program/io.ts`, `cli/test/acceptance/run-lane.ts` | EDIT | Test-only exports and an unreachable fallback go (F007, F144, F348, F353); the lane list derives from the glob (F010) |
| `ui/packages/app/**/*.test.ts`, `ui/packages/app/**/*.test.tsx`, `ui/packages/app/tests/**` | EDIT / DELETE | App rows |
| `ui/packages/app/lib/streaming/install-steps.ts`, `ui/packages/app/lib/streaming/fleet-stream-registry.ts`, `ui/packages/app/lib/streaming/workspace-stream.ts`, `ui/packages/app/components/domain/useFleetPendingSends.ts`, `ui/packages/app/app/cli-auth/[session_id]/page.tsx` | EDIT | Exports and branches only test resets or deleted tests reach (F006, F361–F363, F369; §5) |
| `ui/packages/design-system/src/**/*.test.ts`, `ui/packages/design-system/src/**/*.test.tsx`, `ui/packages/website/src/**/*.test.ts`, `ui/packages/website/src/**/*.test.tsx`, `ui/packages/website/tests/e2e/**` | EDIT / DELETE | Design-system and website rows |
| `playbooks/**/*_test.sh`, `deploy/baremetal/service_test.sh`, `ui/agentsfleet.dev/install_test.sh`, `scripts/*_test.sh`, `scripts/*_test.py` | EDIT / DELETE | Shell and Python rows |
| `playbooks/lib/test_search.sh` and its ten sourcing tests | DELETE / EDIT | The `rg` shim only tests use (F354); callers use `grep -E`/`grep -F` |
| `scripts/check_documentation_rules.py`, `scripts/check_architecture_doc.sh`, `scripts/check_model_allowlist.py`, `ui/packages/{app,design-system,website}/.oxlintrc.json`, `cli/.oxlintrc.json`, `make/quality.mk`, `make/check-safety-gates.mk` | EDIT | Lint hosts for moved checks and their wiring — Indy approves each at PLAN |
| `scripts/check_tests_call_code.py`, `scripts/check_tests_call_code_test.py` | CREATE | §6 lint and its seeded-violation tests — Indy approves at PLAN |
| `docs/architecture/testing.md` | EDIT | The rule: a test calls the code; text rules over source are lints |

## Applicable Rules

- **`.orly/docs/greptile-learnings/RULES.md`** — A test that cannot fail is not a test (TCF): every rewrite is made red once by breaking its subject. No Dead Code (NDC) and no helpers without a consumer (HLP): test-only exports go with their tests. No Legacy Retained (NLR) in every file opened. Orphan sweep (ORP) and its close-out gate (CHR) on every deleted symbol. Milestone-free test names (TST-NAM) and names that state what they verify (TNM): rewrites drop `m201`/`m202_001` prefixes. Fixtures use production constants (TFX). Standard parsers (PSR) in every new lint. Literals become named constants (UFS).
- `.orly/dispatch/write_rust.md`, `.orly/dispatch/write_ts_adhere_bun.md`, `.orly/dispatch/write_shell.md`, `.orly/dispatch/write_python.md`, `.orly/dispatch/write_any.md` — fire per file type at EXECUTE.
- `.orly/docs/REST_API_DESIGN_GUIDELINES.md:470` — committed-document freshness is a regenerate-and-diff verify step; tests read the generated document.
- `.orly/dispatch/verify.md`, `.orly/dispatch/lifecycle.md` — package runners never satisfy a lane claim; red-run evidence goes in Session Notes.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| File & Function Length (≤350/≤50/≤70) | Yes | Live rewrites get their own `integration_*` files; deletions only shrink files |
| UFS / LOGGING / RUST ERR | Yes, on new test and lint code | Named constants; no new fallible production signatures |
| MILESTONE-ID | Yes | No `M217` or other milestone identifiers in test names, files or lint output |
| DESIGN TOKEN / UI | Yes, design-system and app tests | Test edits only; no class or token changes in components |
| Harness and lint edits (`.orly/AGENTS.md` §Hard Safety) | Yes | Every `make/*.mk`, `.oxlintrc.json` and `scripts/check_*` edit, and §6, waits for Indy's in-session approval at PLAN; `.orly/` stays untouched |
| Continuous Integration and Deployment (CI/CD), `.github/workflows/**` | No | The signup spec runs through `deploy-dev-acceptance.yml` as it stands |
| SPEC TEMPLATE | Yes | `bash .orly/audits/spec-template.sh --staged` on every spec edit |

## Prior-Art / Reference Implementations

- **Generated schema:** `rustd/crates/afd_wire/tests/validation_holds.rs:122` reads `<T as utoipa::PartialSchema>::schema()`; `openapi_codes.rs:98` walks `afd_api::openapi::document()`. Every OpenAPI rewrite mirrors these.
- **Log absence:** `afd_core::test_util::trace::Capture` (`rustd/crates/afd_core/src/test_util/trace.rs:97`) proves a handler logs no email (F051).
- **Mounted set:** `rustd/crates/afd_api/tests/route_verbs.rs:77` builds each route's real mount; it replaces the router test's private matcher.
- **Lint shape:** `scripts/check_documentation_rules.py` with `check_documentation_rules_test.py`, run from `make/quality.mk:7`.

## Sections (implementation slices)

### §1 — Tests that can never fail or never run (F001–F009)

First, because each one reports a guarantee nobody holds. All verified on `main`: `afd_core` builds one harness (`Cargo.toml:4` `autotests = false`, `core_suite.rs:11` `mod backtrace;`), so `--exact` with a bare name (`backtrace.rs:44`) matches nothing and the child exits 0; `router.rs:195` makes `is_mounted` true for every route family; `gallery/sql.rs:168` pairs both ranks with `FIRST_PAGE`; nothing sets the flag `signup-webhook-live.spec.ts:38` skips on; `make/test-integration-rustd.mk:139,141` and the coverage shards (`:323`) all exclude `afd_bench`, so no lane runs its `#[ignore]`d tests.

- **Dimension 1.1** — The four re-exec children (`afd_core` backtrace; `afd_crypto`, `afd_db`, `afd_dragonfly` error surfaces) filter by their module-qualified names, and each parent asserts its child ran exactly one test → Test `should_render_the_backtrace_only_when_the_environment_asks_for_one`
- **Dimension 1.2** — `test_every_tabled_route_is_served` goes; the full-router loop keeps its not-404 check and loses `is_mounted`, its helpers and the unreachable arm → Test `test_declared_verbs_match_the_mounted_router`
- **Dimension 1.3** — The gallery's statement-text tests go; a live test proves tier order across a page boundary → Test `the_gallery_orders_a_tied_instant_by_tier_across_pages`
- **Dimension 1.4** — The `E2E_LIVE_SIGNUP_WEBHOOK` opt-in goes; the local and production skips stay → Test `Clerk user.created webhook provisions workspace without harness bootstrap`
- **Dimension 1.5** — The assertion-free unmount pair, the length tautology with `CLI_CREDENTIAL_TOTAL_LEN`, and the two website checks that cannot fail are deleted → Lane `make test-coverage-all`
- **Dimension 1.6** — 21 of the 22 ignored `afd_bench` tests go; the one that dials `127.0.0.1:1` runs un-ignored in the unit lane → Test `test_a_run_that_cannot_reach_its_datastore_writes_no_result`

### §2 — Tests that read source, configuration, workflow or documentation (F010–F116)

107 sites (124 findings). `execute` rewrites run the code; `delete` and `trim` rows already have an executing twin, named in the ledger's last column. A split row resolves one way: execute beats lint; delete beats lint when the twin exists (F097). **Implementation default:** a `lint` row moves to the host for its surface: a standard tool's rule first (oxlint `no-restricted-imports`/`no-restricted-syntax`, actionlint, shellcheck); otherwise the existing repository script for that surface (`check_documentation_rules.py` for OpenAPI and command prose, `check_architecture_doc.sh` for architecture tables, `check_model_allowlist.py`, `check-playbooks`, `check-deploy-safety`). A Rust parity scan becomes a compile-time check where one exists (generate `MIGRATIONS` from `schema/`). A check whose natural host is an `orly`-managed audit gets a repository-owned host instead. **Required human decision:** Indy approves the host list at PLAN; a lint-routed test is deleted only in the commit that lands its rule.

- **Dimension 2.1** — Rust statement-text tests (F017, F018, F020–F024, F026–F033) become live tests through the store, each red with its guarded predicate removed → Test `the_vault_never_reads_another_workspaces_key`
- **Dimension 2.2** — OpenAPI readers (F019, F025, F034, F035, F038) read the generated document; `workspace-client.test.ts`'s interface facts move into `afd_api`'s OpenAPI tests → Test `test_the_published_catalog_id_bound_is_the_enforced_one`
- **Dimension 2.3** — CLI, app, design-system, shell and handler rewrites (F010–F016, F036, F037, F039, F051) drive the code: login inspects what it saved, scripts run against stubbed tools, the identity handler runs under `Capture` → Test `login persists the minted credential, never the decrypted session token`
- **Dimension 2.4** — Every `lint` row in §2 and §3 lands with a seeded violation in its host's own test, and the host exits non-zero naming it → Test `moved_rules_reject_seeded_violations`
- **Dimension 2.5** — `delete` and `trim` rows go and their named twins still fail on the behaviour → Lane `make test-coverage-all`

### §3 — Wording and class-name pins (PIN), then duplicates (DUP) (F117–F336)

220 sites. A pin on observable interface stays asserted by execution: error codes, wire spellings, exit codes, accessible names. Pins on copy, class strings, icon names and constants compared with themselves go or trim to their behavioural half. For each duplicate the stronger twin named in the ledger stays.

- **Dimension 3.1** — Copy and class pins (PIN `delete`/`trim`) go; interface assertions stay → Lane `make test-coverage-all`
- **Dimension 3.2** — Rewrites (F117, F118, F200–F206) observe behaviour: a client-side round-trip keeps one stream, and queries go by role and accessible name → Test `fleet thread survives a client-side round-trip without remounting`
- **Dimension 3.3** — The weaker twin goes; trims keep the unique half, such as the absent-event 404 (`workspace_events.rs:279`) → Test `the_absent_event_code_is_the_events_own_and_not_the_fleets`

### §4 — Tests of test-only code (DEAD) and of their own mocks (MOCK) (F337–F385)

49 sites. Harness self-tests leave the unit suite. Two safety checks in `clerk-admin.test.ts` stay: `:183` refuses an identity without the suite ownership marker, and `:198` redacts the generated credential. `cli-credential-revoke.test.ts:118` ("gives up at the attempt cap, naming the id and every attempt, never the secret") also stays; its `:130` keeps a minted credential out of logs.

- **Dimension 4.1** — Fixture and harness tests go; the three safety checks above stay → Test `refuses an existing identity without the suite ownership marker`
- **Dimension 4.2** — The `AgentRun` redaction test moves beside `AgentRun` before the `ScriptedEngine` tests go (F345) → Test `a_run_debugs_without_the_leases_secrets`
- **Dimension 4.3** — Harness checks (F338–F342) run in the lane that uses them: the `compose-env` leak guard as an acceptance preflight, `bench_datastore_test.py` in `make lint-all` → Test `CompleteEvidencePasses`
- **Dimension 4.4** — Mock-only tests go; the rewrites (F337, F372–F375) assert on the system under test → Test `a_workspace_that_does_not_exist_is_not_distinguishable_from_a_foreign_one`
- **Dimension 4.5** — Test-only exports and reset-only branches go with their tests (§5) → Lane `make test-coverage-all`

### §5 — Coverage holds without moving a floor

Runs at every Section's commit. A production line a deletion leaves uncovered gets an executing test through its public entry if a production caller reaches it, or is deleted if none does. No threshold moves, no `RUSTD_COVERAGE_IGNORE` pattern or coverage-ignore comment is added, and no file joins a coverage exclude.

- **Dimension 5.1** — TypeScript floors hold at 100% with threshold sources unchanged → Lane `make test-coverage-all`
- **Dimension 5.2** — The Rust floor holds at `RUSTD_COVERAGE_FLOOR ?= 97.5` (`make/test-integration-rustd.mk:212`) → Lane `make test-coverage-rustd`

### §6 — The rule, enforced (requires Indy's approval)

**Required human decision:** Indy approves the new check and its `make lint-all` wiring at PLAN. Declined → amend this spec to drop §6, rubric R2 and the Goal's first clause, and record the invariant as review-enforced in Discovery.

- **Dimension 6.1** — `scripts/check_tests_call_code.py` flags a file read of a repository path from a test file outside that test's fixture directories, in TypeScript, Rust, shell and Python, with no allowlist or baseline → Test `test_a_seeded_source_read_fails_in_each_language`
- **Dimension 6.2** — `make lint-all` runs the check and fails on a seeded violation → Test `lint_all_runs_check_tests_call_code`

## Interfaces

```
python3 scripts/check_tests_call_code.py
  exit 0 clean · 1 findings · 2 usage error
  stdout per finding: <path>:<line>: test reads <target>; call the code or move the check to a lint
Ledger row: | ID | Site | Tests | Action | Why it does not prove behaviour | If removed |
Unchanged: HTTP routes, CLI verbs and flags, the OpenAPI document, wire types, every coverage threshold.
```

## Failure Modes

| Mode | Cause | Handling (system response + what the caller observes) |
|------|-------|--------------------------------------------------------|
| Lost guarantee | A `delete` row turns out to be the only proof of real behaviour | The row becomes `execute` or `lint`, noted in Discovery; its replacement lands in the deleting commit; `moved_rules_reject_seeded_violations` |
| Coverage drop | A deletion exposes production code | Tested or deleted per §5; a threshold edit fails rubric R3 and `make test-coverage-all` |
| Stale ledger | M213 or a later commit moves or removes a site | Re-located by test name at CHORE(open); a vanished row is marked with its commit, never dropped silently |
| Vacuous rewrite | A rewritten test passes with its subject broken | Red-run per `execute` row recorded in Session Notes (TCF); `the_vault_never_reads_another_workspaces_key` |
| Lint false positive | The check flags a fixture read | Matcher fixed and a negative case added; no allowlist entry; `test_a_seeded_source_read_fails_in_each_language` |
| Host not approved | Indy declines a lint host | The row's test stays until a host is approved; recorded in Discovery |
| Live test interference | New integration tests share one datastore | ISO-1 to ISO-3 from `testing.md`; per-run names; `the_gallery_orders_a_tied_instant_by_tier_across_pages` |
| Signup run leaves a user | The live spec fails mid-flow | Its `afterEach` deletes the created user; failure shows in `deploy-dev-acceptance.yml` |

## Invariants

1. No test reads a repository file outside its own fixture directories — `scripts/check_tests_call_code.py` in `make lint-all` (§6, after approval).
2. Coverage floors never move — rubric R3 diffs the five threshold sources and scans added lines for ignore comments.
3. No `orly`-managed file changes — rubric R4 (`git diff` under `.orly/` is empty).
4. Every moved rule rejects a seeded violation — its host's own test.
5. The three harness safety checks survive — rubric R5 counts them.

## Metrics & Observability

| Metric / event | Owner | Fires when | Properties allowed | Privacy guard | Test proof |
|----------------|-------|------------|--------------------|---------------|------------|
| not applicable — no product/operator signal changes | not applicable | never | none | no credential or email appears in lint output or test names | lane `make test-coverage-all` |

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `should_render_the_backtrace_only_when_the_environment_asks_for_one` | Child stdout reports `1 passed`; breaking backtrace rendering fails the parent; same for the three error-surface twins |
| 1.2 | unit | `test_declared_verbs_match_the_mounted_router` | A route with its handler unmounted → `expect` panics naming the route; `is_mounted` absent from `router.rs` |
| 1.3 | integration | `the_gallery_orders_a_tied_instant_by_tier_across_pages` | Platform and tenant rows at one instant across a page boundary → platform first on both pages; wrong rank in `PAGE_AFTER` → red |
| 1.4 | e2e | `Clerk user.created webhook provisions workspace without harness bootstrap` | `deploy-dev-acceptance.yml` run → status passed, never skipped; local run still skips |
| 1.5 | unit | lane `make test-coverage-all` | `make test-coverage-all` exit 0; the `cancelled` guard is asserted by observation or deleted |
| 1.6 | unit | `test_a_run_that_cannot_reach_its_datastore_writes_no_result` | Runs in `make test-unit-all` without `--ignored`; no other `#[ignore]` remains in `afd_bench` |
| 2.1 | integration | `the_vault_never_reads_another_workspaces_key` | Two workspaces, one key name → each read returns its own row; workspace predicate removed → red |
| 2.2 | unit | `test_the_published_catalog_id_bound_is_the_enforced_one` | Bound from `document()` equals the handler's constant; no `public/openapi.json` read in any test |
| 2.3 | unit | `login persists the minted credential, never the decrypted session token` | Device flow driven to success → `saveAccessToken` receives the minted credential; swapping in the session token → red |
| 2.4 | unit | `moved_rules_reject_seeded_violations` | One violating fixture per moved rule → host exits 1 naming it; clean tree → exit 0 |
| 2.5 | unit | lane `make test-coverage-all` | Each deleted row's named twin exists; `make test-coverage-all` exit 0 |
| 3.1 | unit | lane `make test-coverage-all` | No copy or class-string assertion left in ledger PIN rows; interface assertions present |
| 3.2 | e2e | `fleet thread survives a client-side round-trip without remounting` | Sidebar click away and back → one stream open, no remount; full-page navigation → red |
| 3.3 | unit | `the_absent_event_code_is_the_events_own_and_not_the_fleets` | Absent event → 404 with the event's own code; the duplicate `assert_ne` half gone |
| 4.1 | unit | `refuses an existing identity without the suite ownership marker` | Identity without the marker → refused, untouched; redaction and revoke-log checks also present |
| 4.2 | unit | `a_run_debugs_without_the_leases_secrets` | `format!("{run:?}")` holds no lease API key; lives outside `scripted/tests.rs` |
| 4.3 | unit | `CompleteEvidencePasses` | `make lint-all` output names the grader's self-test; acceptance preflight refuses a leaked parent `AGENTSFLEET_API_KEY` |
| 4.4 | unit | `a_workspace_that_does_not_exist_is_not_distinguishable_from_a_foreign_one` | The stub answers a foreign workspace differently from an absent one → both still answer the same 404 |
| 4.5 | unit | lane `make test-coverage-all` | `fingerprintHex`, `workspacesFromValueLayer`, `isInstallFrame` absent; coverage gate exit 0 |
| 5.1 | unit | lane `make test-coverage-all` | Rubric R3 → no threshold file changed, no ignore comment added; `make test-coverage-all` exit 0 |
| 5.2 | integration | lane `make test-coverage-rustd` | `make test-coverage-rustd` exit 0 at floor 97.5; `RUSTD_COVERAGE_IGNORE` unchanged |
| 6.1 | unit | `test_a_seeded_source_read_fails_in_each_language` | A source read seeded in TypeScript, Rust, shell and Python → exit 1 each; a fixture read → exit 0 |
| 6.2 | unit | `lint_all_runs_check_tests_call_code` | A seeded violation → `make lint-all` exits non-zero naming the file; removed → exit 0 |

Regression: product behaviour, public interfaces and every surviving test stay green. 13 of the 23 rows above prove a refusal or a red-run.

## Acceptance Rubric (single scoring surface)

| # | Criterion (observable outcome) | Verify (copy-paste) | Expected | Priority | Graded (VERIFY) |
|---|--------------------------------|---------------------|----------|----------|-----------------|
| R1 | §1 traps are gone (§1) | `git grep -nE 'fn test_every_tabled_route_is_served\|E2E_LIVE_SIGNUP_WEBHOOK\|CLI_CREDENTIAL_TOTAL_LEN\|const fn is_mounted' -- rustd ui cli \| wc -l` | `0` | P0 | |
| R2 | No test reads a repository file (§2, §6) | `python3 scripts/check_tests_call_code.py` | exit 0 | P0 | |
| R3 | Coverage floors unchanged (§5) | `git diff --name-only origin/main...HEAD -- ui/packages/app/vitest.config.ts ui/packages/design-system/vitest.config.ts ui/packages/website/vite.config.ts cli/bunfig.toml make/test-integration-rustd.mk; git diff -U0 origin/main...HEAD \| grep -cE '^\+.*(v8 ignore\|c8 ignore\|istanbul ignore\|coverage\(off\))'` | no paths, then `0` | P0 | |
| R4 | Scope held; `orly` files and workflows untouched | `git diff --name-only origin/main...HEAD -- .orly/ .github/` | no output; every other changed path matches Files Changed | P0 | |
| R5 | Harness safety checks survive (§4) | `git grep -cE 'refuses an existing identity without the suite ownership marker\|redacts the generated credential when Clerk echoes it\|not.toContain\(CLI_CREDENTIAL\)' -- cli/test/acceptance/fixtures` | counts sum to 3 | P0 | |
| S1 | Conform gates green | `make harness-verify` | exit 0 | P0 | |
| S2 | Unit tests pass | `make test-unit-all` | exit 0 | P0 | |
| S3 | Lint passes | `make lint-all` | exit 0 | P0 | |
| S4 | Integration passes | `make test-integration-rustd` | exit 0 | P0 | |
| S5 | Version in sync | `make check-version` | exit 0 | P0 | |
| S6 | No secrets | `gitleaks protect --staged --redact --no-banner` | exit 0 | P0 | |

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

| File to delete | Verify |
|----------------|--------|
| `playbooks/lib/test_search.sh`, `playbooks/lib/test_search_test.sh` | `test ! -f playbooks/lib/test_search.sh` |
| `rustd/crates/afd_dragonfly/tests/integration_diagnose_connect.rs` | `test ! -f rustd/crates/afd_dragonfly/tests/integration_diagnose_connect.rs` |
| `rustd/crates/afr_supervisor/src/test_support/sandbox_tests.rs` | `test ! -f rustd/crates/afr_supervisor/src/test_support/sandbox_tests.rs` |
| `rustd/crates/afr_tools/src/sandbox/scripted/tests.rs` | `test ! -f rustd/crates/afr_tools/src/sandbox/scripted/tests.rs` |
| `cli/test/helpers-output-double.unit.test.ts` | `test ! -f cli/test/helpers-output-double.unit.test.ts` |

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| `fingerprintHex`, `workspacesFromValueLayer`, `CLI_CREDENTIAL_TOTAL_LEN` | `git grep -nwE 'fingerprintHex\|workspacesFromValueLayer\|CLI_CREDENTIAL_TOTAL_LEN' -- cli/src cli/test` | 0 matches |
| `isInstallFrame` | `git grep -nw isInstallFrame -- ui/packages/app` | 0 matches |
| `build_client_for_diagnosis`, `integration_diagnose_connect`, `sandbox_tests` | `git grep -nwE 'build_client_for_diagnosis\|integration_diagnose_connect\|sandbox_tests' -- rustd` | 0 matches |
| `test_search.sh` | `git grep -nw test_search -- playbooks scripts deploy` | 0 matches |

Every other ledger `delete` row is checked by test name with `git grep -nF` at CHORE(close); the PR body lists any survivor.

## Out of Scope

- The 20 findings the audit's second reviewer rejected, and tests outside the ledger; a new audit is a new spec.
- A lint for substring assertions on production string constants (statement text): §2 removes them, and no reliable signal separates them from legitimate equality checks.
- Edits to `.orly/` files; widening `orly`'s design-token audit to the design-system package is a request to the `orly` repository.
- CI workflow edits, product behaviour, OpenAPI, CLI verbs, and the `~/Projects/docs` repository.
- A regenerate-and-diff gate for `public/openapi.json`; the Representational State Transfer (REST) guide keeps it a verify step.

## Product Clarity (authoring record)

1. **Successful user moment** — A contributor rewords a comment, reflows a file and renames a local; the suite stays green. They break a tenant predicate and a named test goes red.
2. **Preserved user behaviour** — Every product path, command, route and surviving test behaves as before; only tests and test-only code change.
3. **Optimal-way check** — Rewriting where a guarantee is real and deleting the rest is the direct route; the gap is the SQL-pin class, which no lint can see.
4. **Rebuild-vs-iterate** — Iterate on the ledger; a new test framework would not change what the tests assert.
5. **What we build** — Rewritten tests, moved lint rules with seeded fixtures, one source-read lint, and one paragraph in `testing.md`.
6. **What we do NOT build** — A statement-text lint, a coverage-exception mechanism, or a new test runner.
7. **Fit with existing features** — Compounds with `make lint-all` and the coverage gates; must not destabilise the 100% TypeScript floors.
8. **Surface order** — N/A — no user surface; the lint is contributor tooling run through `make`.
9. **Dashboard restraint** — N/A — no product interface changes.
10. **Confused-user next step** — The lint's finding line says to call the code or move the check; `testing.md` explains why.

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** Sections follow Indy's order; then categories, so each commit is reviewable against one rule. §5 rides every commit; §6 lands last, once nothing it would flag remains.
- **Alternatives considered:** One Pull Request per surface splits Indy's risk order across five reviews. A delete-only sweep loses the guarantees the ledger marks real. A baseline-backed lint first would hide 403 findings behind an allowlist.
- **Patch-vs-refactor verdict:** this is a **patch** because each row is a local test change; the larger move, generating lane lists and migration registries from the tree, appears only where a row needs it (F010, F054).
- **Surface-area checklist:** OpenAPI no; product CLI no; user docs no; release/version no; schema no; spec-vs-rules conflict no.

## Discovery (consult log)

- **Consults** — Indy, Oct 08, 2026, on tests that read source files: "I dont think we must read any source" (recorded in M213_001 Discovery on its branch). Indy, Oct 08, 2026, asked "The repo-wide audit found 409 tests across 337 files that read source, pin wording, duplicate other tests, never run, or test their own mocks. Write a new milestone spec for them, landing after M213?", answered: "New spec after M213 (Recommended)". The Section order and the `clerk-admin.test.ts` keepers were relayed by the session agent, not quoted. The relayed keeper "never delete outside the test org" matches no test on `main` (`grep -ci org cli/test/acceptance/fixtures/clerk-admin.test.ts` → `0`), so §4 keeps the audit's pair (`:183`, `:198`); Indy confirms at PLAN. Audit run on `feat/m213-rust-runner-cutover` at `7fc1a7ac5`; ledger verified against `main` at `2a21591b3`. M213 owns F386–F391.
- **Overnight authorization** — Indy, Oct 10, 2026, starting this stream before going offline: "ensure we process thorugh to a PR … follow governance process, gstack /review (fix all), push PR, watch CI jobs make it green, fix greptile feedback". Then, mid-session: "No doc updates needed = override approved in orly, and other overrides as you need for orly. You can inform Indy in the mornign, indy can review and propose changes later." The session agent read the second message as approval of the PLAN-time decisions this spec reserves (the lint host list, §6 and its `make lint-all` wiring, the `clerk-admin.test.ts` keepers at `:183`/`:198`), each landing in its own commit so Indy can drop it, and as the `orly gate pr` documentation override (no `~/Projects/docs` change). The morning report lists each decision.
- **Coverage bar for touched code** — Indy, Oct 10, 2026: "ensure that if you touch rust code the coverage must be 99%, typescript the coverage is 100%". Every Rust production file this branch changes ends at ≥99% line coverage with every added or changed line covered; TypeScript packages stay at 100%. `RUSTD_COVERAGE_FLOOR` (97.5) stays unedited: the bar applies to touched files, and §5 forbids moving a threshold source.
- **M213-owned rows** — F386–F391 no longer exist on the branch base: `git grep -lF` on each row's first test name returns no file.
- **Metrics review** — No analytics/funnel playbook update required: no product or operator event changes.
- **Skill-chain outcomes** — `orly-spec-new` applied at authoring. `/orly-write-unit-test`, gstack `/review` and `orly-babysit-prs` pending.
- **Deferrals** — None.
