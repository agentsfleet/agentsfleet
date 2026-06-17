# M93_002: Drive Command-Line Interface coverage to 100 percent without hiding behavior

**Prototype:** v2.0.0
**Milestone:** M93
**Workstream:** 002
**Date:** Jun 17, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — the Command-Line Interface (CLI) is the operator entrypoint; coverage gaps block release confidence.
**Categories:** CLI
**Batch:** B1 — standalone hardening; independent of M93_001 and the M92_001 website refresh.
**Branch:** feat/m93-cli-coverage-hardening
**Test Baseline:** unit=1951 integration=189
**Depends on:** M92_003 (the CLI package and binary rename already landed)
**Provenance:** agent-generated (coverage audit, Jun 17, 2026) — grounded in `cli/bunfig.toml`, `cli/coverage/lcov.info`, and the reference path `~/Projects/oss/cli/apps/cli/src/next`.

> **Provenance is load-bearing.** The coverage numbers must be re-created from a fresh `npm run test:coverage` before implementation. Treat this spec's coverage ledger as a starting observation, not stale truth.

**Canonical architecture:** `docs/TEMPLATE.md` Prior-Art / CLI section plus the actual CLI runtime split under `cli/src/program/`, `cli/src/runtime/`, and `cli/src/services/`. No separate CLI architecture doc exists in this checkout.

## Implementing agent — read these first

1. `cli/bunfig.toml` — current JavaScriptCore (JSC) coverage rationale and the existing tag-only ignore pattern.
2. `cli/scripts/enforce-coverage.mjs` — current threshold enforcement and output parsing.
3. `cli/src/program/handlers-bind.ts` + `cli/src/lib/run-effect.ts` + `cli/src/runtime/main-layer.ts` — the existing Commander-to-Effect bridge and service composition pattern.
4. `~/Projects/oss/cli/apps/cli/src/next` — reference for service/tag layout and handler-first Effect shape.
5. `cli/test/*coverage*`, `cli/test/*linecov*`, and `cli/test/telemetry/*.unit.test.ts` — existing coverage backfill style; preserve the good assertions, remove misleading comments when code changes make them stale.

## PR Intent & comprehension handshake

- **Pull Request (PR) title (eventual):** harden CLI coverage to 100 percent
- **Intent (one sentence):** The CLI coverage command reports 100 percent lines and 100 percent functions over executable CLI behavior, while tag-only JSC artifacts are isolated so the metric reflects behavior instead of instrumentation noise.
- **Handshake (agent fills at PLAN, before EXECUTE):** restate intent + `ASSUMPTIONS I'M MAKING: ...`; reconcile any mismatch before editing.

## Product Clarity (answer in order, at authoring)

1. **Successful user moment** — an operator or reviewer runs `cd cli && npm run test:coverage` and sees `All files | 100.00 | 100.00`, with tests proving login, retry, structured errors, agent terminology, and renderer behavior still work.
2. **Preserved user behaviour** — every CLI command, JSON shape, human output, exit code, retry rule, and auto-JSON-when-piped behavior remains unchanged unless a test exposes an existing bug.
3. **Optimal-way check** — the unconstrained-optimal is a coverage engine that credits TypeScript and Effect service tags correctly. The available engine is Bun/JSC, so the practical shape is to keep executable behavior in covered files and move tag-only dependency-injection declarations into ignored tag files.
4. **Rebuild-vs-iterate** — iterate. The CLI is already mostly Effect-backed; replacing Commander or rewriting the command tree is not required to hit coverage.
5. **What we build** — targeted tests for every uncovered line, small refactors that make defensive branches reachable or remove unreachable code, tag-only file splits for JSC-only function gaps, and a 100/100 enforcement gate.
6. **What we do NOT build** — a Commander replacement; new CLI commands; API changes; docs-site copy; website changes; package upgrades.
7. **Fit with existing features** — compounds with the M74 Effect migration and M92 binary rename; must not destabilize login, retry, telemetry, memory read verbs, or agent lifecycle commands.
8. **Surface order** — CLI-first only.
9. **Dashboard restraint** — N/A; no UI surface.
10. **Confused-user next step** — the coverage failure output names the file and line from Bun's table; the spec's Discovery ledger records each remaining gap and how it was closed.

## Applicable Rules

- **`docs/greptile-learnings/RULES.md`** — specifically No Dead Code (NDC), No Legacy Retained (NLR), CLI JSON discipline (JCL), Error Class distinction (ECL), Standard Parser (PSR), and Test Naming (TST-NAM).
- **`dispatch/write_ts_adhere_bun.md`** — TypeScript/Bun authoring, Effect service layout, `bun:test` style, resource cleanup, and no default exports for application code.
- **`dispatch/write_any.md`** — File and Function Length (FLL), Unified Form for Symbols (UFS), logging discipline, milestone-free source identifiers, and greptile rule audit.
- **`dispatch/verify.md`** — final pass must use repo-level verification, not only package-local commands.

## Applicable Gates

| Gate | Fires? | Satisfaction strategy |
|------|--------|-----------------------|
| ZIG GATE | no | No Zig files. |
| PUB / Struct-Shape | no | No Zig public structs. |
| File & Function Length | yes | Keep every touched TypeScript file under caps; split tests instead of growing catch-all fillers. |
| UFS | yes | New semantic strings become constants when repeated; avoid duplicating CLI error codes. |
| UI Substitution / DESIGN TOKEN | no | No UI files. |
| LOGGING | yes | No new console output in source; tests may capture streams. |
| ERROR REGISTRY | yes | Any changed CLI-visible code must preserve `UZ-*` server codes and local `ERR_*` codes. |
| SCHEMA | no | No schema files. |

## Overview

**Goal (testable):** `cd cli && npm run test:coverage` reports `All files | 100.00 | 100.00`, and `cd cli && npm run test` enforces the same floor.

**Problem:** The CLI coverage gate currently passes at 97 percent functions and 99 percent lines, but the report is not literal 100 percent. The fresh baseline was 1,154 pass / 2 skip / 0 fail with `All files | 97.61 | 99.55`. Remaining gaps include real missed lines plus JSC function accounting around Effect `Context.Service` tag classes and higher-order callbacks.

**Solution summary:** Close real behavior gaps with focused unit/integration tests. Refactor unreachable defensive code into reachable, simpler expressions where behavior is unchanged. Move tag-only dependency-injection declarations into ignored tag files matching the existing pattern in `cli/bunfig.toml`, so executable service implementations remain covered and JSC artifacts stop depressing the function metric. Raise enforcement to 100/100 only after the report is clean.

## Prior-Art / Reference Implementations

- **Reference CLI** → `~/Projects/oss/cli/apps/cli/src/next`: mirror its separation of handler logic, service tags, layers, and runtime wiring where local code already follows that direction.
- **Existing local pattern** → `cli/bunfig.toml` already ignores tag-only `.service.ts` files for browser, analytics, AI-tool, and runtime services; extend that pattern only to files that contain no executable behavior.
- **Test style** → `cli/test/http-retry.unit.test.ts`, `cli/test/login-device-flow.unit.test.ts`, and `cli/test/telemetry/*.unit.test.ts`: behavior-first tests with deterministic failure injection.

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `cli/bunfig.toml` | EDIT | Raise coverage floor to 100/100 and ignore only tag-only files. |
| `cli/scripts/enforce-coverage.mjs` | EDIT | Parse and enforce exact 100/100 output; keep failure text actionable. |
| `.github/workflows/test.yml` | EDIT | Remove Codecov upload steps while preserving package coverage commands in the GitHub test workflow. |
| `cli/src/cli.ts` | EDIT | Remove or make reachable the remaining defensive Commander error mapping line without changing public exit codes. |
| `cli/src/errors/auth.ts` | EDIT | Replace factory shape if JSC keeps mis-mapping executable error classes to type-only lines. |
| `cli/src/errors/index.ts` | EDIT | Preserve error variants while eliminating uncredited class/function artifacts if needed. |
| `cli/src/runtime/*` | EDIT/CREATE | Split tag-only declarations from executable helpers where needed. |
| `cli/src/services/*` | EDIT/CREATE | Split tag-only declarations from executable implementations; keep imports stable through re-exports where practical. |
| `cli/src/lib/browser.ts` | EDIT/TEST | Cover default command probe path or simplify unreachable helper shape. |
| `cli/src/lib/sse.ts` | EDIT/TEST | Cover external abort listener and non-2xx error parsing fallbacks. |
| `cli/src/lib/stream-fetch.ts` | TEST | Cover unavailable fetch and null-body branches. |
| `cli/src/lib/http-retry.ts` | TEST | Cover retry exhaustion loop close / terminal emit path. |
| `cli/src/program/validators.ts` | TEST | Cover non-string JSON option validation. |
| `cli/src/services/telemetry/*` | EDIT/TEST | Cover `Option.none` flag normalization, tracing-only path, and runtime distinct-id branch accounting. |
| `cli/src/commands/*` | TEST | Cover JSON delete/add branches and login verification warning branches. |
| `cli/test/**/*.ts` | EDIT/CREATE | Focused behavior tests; no assertion-free coverage padding. |

## Decomposition & alternatives (patch vs refactor)

- **Chosen shape:** one section for the coverage ledger, one for real branch tests, one for tag-only JSC isolation, one for enforcement, one for CLI error/retry terminology audit.
- **Alternatives considered:** (a) ignore whole files with artifacts — rejected because it hides real behavior; (b) add no-op constructor tests — rejected because it proves nothing; (c) replace Commander with the reference Effect CLI runner — rejected as larger than the coverage goal.
- **Patch-vs-refactor verdict:** targeted refactor. Tests close behavior gaps; tag splits are structural but narrow and follow the existing local pattern.

## Sections (implementation slices)

### §1 — Coverage ledger from a fresh baseline

Re-run coverage, record every file below 100 percent, and classify each gap as real behavior, unreachable defensive code, or JSC tag artifact.

- **Dimension 1.1** — fresh baseline captured from `cd cli && npm run test:coverage` → Test/evidence `coverage_baseline_table`
- **Dimension 1.2** — every gap has a classification and close action → Test/evidence `coverage_gap_ledger`

### §2 — Real behavior gaps closed by tests

Add focused tests through public command/effect surfaces for every missed line in command handlers, validators, retry transport, stream transports, and telemetry helpers.

- **Dimension 2.1** — CLI command JSON branches covered (`credential add`, `agent-key delete`, `grant delete`) → Tests in command/effect suites
- **Dimension 2.2** — login verification retry warning and malformed-code prompt branch covered → Tests in login device-flow suite
- **Dimension 2.3** — stream fetch and stream get failure branches covered → Tests in stream suites
- **Dimension 2.4** — validator non-string JSON option branch covered → Test in validators suite
- **Dimension 2.5** — retry exhaustion / terminal emit path covered without sleeping → Test in HTTP retry suite

### §3 — JSC tag artifacts isolated without hiding executable behavior

Move pure `Context.Service` tag declarations into tag-only files that `bunfig.toml` ignores. Covered files keep executable helpers, layer constructors, and request logic.

- **Dimension 3.1** — every ignored file is tag-only and has no branch/function behavior → Test/evidence `ignored_tag_files_are_tag_only`
- **Dimension 3.2** — service implementations remain in coverage and report 100 percent lines/functions → Test/evidence `service_coverage_table`
- **Dimension 3.3** — imports remain stable or are updated mechanically with typecheck green → Test `npm run typecheck`

### §4 — Enforcement raised to 100/100

Update coverage settings and enforcement so both package-local and repo-level gates fail below literal 100 percent.

- **Dimension 4.1** — `cli/bunfig.toml` threshold is 1.00 line and 1.00 function → Test/evidence grep
- **Dimension 4.2** — `npm run test` fails below 100 and passes at 100 → Test/evidence command output
- **Dimension 4.3** — `make test-coverage-all` still passes after the CLI floor change → Test/evidence repo command output
- **Dimension 4.4** — GitHub test workflow keeps package coverage gates but has no external coverage upload action → Test/evidence `actionlint .github/workflows/test.yml` + workflow grep

### §5 — Error/retry terminology audit stays green

While touching CLI tests, verify the user-facing concerns that triggered this work: no stale zombie wording in CLI source/tests, agent-facing not-found text, meaningful auth/login error codes, and robust retry coverage.

- **Dimension 5.1** — `rg "zombie|usezombie"` over `cli/src cli/test` returns no live stale user-facing hits → Test/evidence grep
- **Dimension 5.2** — agent not-found paths surface `Agent not found` or agent-specific suggestions, not zombie wording → Tests in agent/memory suites
- **Dimension 5.3** — login/auth/server failure codes keep meaningful `UZ-AUTH-*`, `UZ-AGT-*`, `UZ-MEM-*`, local `ERR_*` shape → Tests in failure-mode suites
- **Dimension 5.4** — retry tests prove 408/429/5xx/network retry, Retry-After, jitter cap, environment opt-out, and non-idempotent mutation safety → Existing + added retry tests

## Interfaces

```
CLI coverage command:
  cd cli && npm run test:coverage
  expected summary: All files | 100.00 | 100.00

CLI enforcement command:
  cd cli && npm run test
  expected: build succeeds, enforce-coverage prints actual function=100.00% line=100.00%, exits 0

Repo coverage command:
  make test-coverage-all
  expected: CLI, app, website, and design-system coverage gates pass
```

No CLI command syntax, JSON output, HTTP API, or state-file format changes.

## Failure Modes

| Mode | Cause | Handling (system response + observable) |
|------|-------|------------------------------------------|
| Coverage padding | Test executes code without asserting behavior | Reject; each test names the bug/behavior it catches. |
| Real behavior hidden | `coveragePathIgnorePatterns` hides executable source | Reject; only tag-only files may be ignored and a grep/audit proves it. |
| Function metric artifact remains | JSC still counts an uncreditable tag/function in an executable file | Move only the artifact into a tag-only file, or simplify code shape without losing behavior. |
| CLI output drift | Tests change snapshots or messages casually | Fail existing golden/output tests; update only with a stated behavior reason. |
| Retry safety regression | New retry test accidentally replays unsafe mutations | Existing POST/PATCH non-idempotent tests stay green; add regression if needed. |
| Stale terminology returns | Agent-facing errors mention zombie/usezombie | grep guard and error tests fail. |

## Invariants

1. The CLI coverage report is literal 100 percent for executable files — enforced by `npm run test` and `npm run test:coverage`.
2. Ignored coverage paths are tag-only — enforced by a test or script that rejects branches, function bodies, fetches, filesystem calls, and command logic in ignored files.
3. Public CLI behavior is unchanged — enforced by existing acceptance, golden output, failure-mode, and command matrix tests.
4. Retry policy remains Supabase-like: safe/idempotent retries only for mutation hazards — enforced by HTTP retry unit and integration tests.
5. Stale zombie wording stays out of CLI source/tests except historical docs outside this spec — enforced by grep evidence in Verification Evidence.

## Test Specification (tiered)

| Dimension | Tier | Test | Asserts (concrete inputs → expected output) |
|-----------|------|------|---------------------------------------------|
| 1.1 | unit | `coverage_baseline_table` | fresh coverage output recorded in Discovery before edits |
| 1.2 | unit | `coverage_gap_ledger` | every below-100 file classified as behavior / unreachable / JSC artifact |
| 2.1 | unit | command JSON branch tests | JSON mode prints expected payload for previously missed command branches |
| 2.2 | unit | login verification retry tests | wrong/malformed verification input warns, retries, and preserves typed errors |
| 2.3 | unit | stream failure tests | unavailable fetch, null body, external abort, and error envelopes map to typed errors |
| 2.4 | unit | validator JSON option non-string | non-string option throws `InvalidArgumentError("must be a string of JSON")` |
| 2.5 | unit | retry terminal path | retry exhaustion emits terminal attempt and rethrows original error |
| 3.1 | unit | `ignored_tag_files_are_tag_only` | ignored files contain only imports/types/tag class declarations/re-exports |
| 3.2 | unit | `service_coverage_table` | service implementation files report 100/100 |
| 3.3 | unit | `npm run typecheck` | imports and exported service symbols remain valid |
| 4.1 | unit | coverage threshold grep | `coverageThreshold = { line = 1, function = 1 }` or equivalent |
| 4.2 | integration | `npm run test` | package build + coverage enforcement passes at 100 |
| 4.3 | integration | `make test-coverage-all` | repo coverage gate passes with CLI at 100 |
| 4.4 | unit | workflow coverage-upload grep | `.github/workflows/test.yml` still runs coverage commands and contains no Codecov upload action |
| 5.1 | unit | stale terminology grep | no live `zombie`/`usezombie` hits under `cli/src cli/test` |
| 5.2 | integration | agent not-found tests | `UZ-AGT-*` / `UZ-MEM-*` errors use agent wording and suggestions |
| 5.3 | integration | failure-mode error-code tests | login/auth/agent/memory/server failures surface stable codes |
| 5.4 | unit/integration | retry policy tests | retry/backoff/safety matrix remains covered |

**Regression:** all existing CLI unit, integration, acceptance, golden-output, retry, login, and telemetry suites pass. **Idempotency/replay:** retry tests must prove POST/PATCH 5xx are not replayed while safe/idempotent methods retry.

## Acceptance Criteria

- [ ] CLI coverage is literal 100/100 — verify: `cd cli && npm run test:coverage`
- [ ] CLI enforcement fails below 100 and passes at 100 — verify: `cd cli && npm run test`
- [ ] Repo coverage stays green — verify: `make test-coverage-all`
- [ ] GitHub test workflow has no external coverage upload action — verify: `actionlint .github/workflows/test.yml` and `rg -n "[Cc]odecov|Upload .*coverage|coverage/lcov.info|CodeQL|codeql" .github/workflows && echo "FAIL" || echo "PASS"`
- [ ] CLI stale terminology grep is clean — verify: `rg -n "zombie|usezombie" cli/src cli/test`
- [ ] TypeScript typecheck and lint pass — verify: `cd cli && npm run typecheck && npm run lint`
- [ ] No hidden real behavior in ignored coverage files — verify: tag-only audit test/script
- [ ] `gitleaks detect` clean · no file over 350 lines added

## Eval Commands (post-implementation)

```bash
# E1: CLI coverage is literal 100
cd cli && npm run test:coverage

# E2: CLI enforced test gate
cd cli && npm run test

# E3: CLI lint and typecheck
cd cli && npm run lint && npm run typecheck

# E4: Repo coverage gate
make test-coverage-all

# E5: Stale CLI terminology sweep
rg -n "zombie|usezombie" cli/src cli/test && echo "FAIL" || echo "PASS"

# E6: Gitleaks
gitleaks detect

# E7: 350-line gate
git diff --name-only origin/main | grep -v '\.md$' | xargs wc -l 2>/dev/null | awk '$1 > 350 {print "OVER: "$2": "$1}'

# E8: GitHub workflow keeps coverage tests but removes external upload
actionlint .github/workflows/test.yml
rg -n "[Cc]odecov|Upload .*coverage|coverage/lcov.info|CodeQL|codeql" .github/workflows && echo "FAIL" || echo "PASS"
```

## Dead Code Sweep

**1. Orphaned files — deleted from disk and git.**

N/A — no planned source deletion. Tag splits create files and leave old imports re-exported or mechanically updated.

**2. Orphaned references — zero remaining imports/uses.**

| Deleted symbol/import | Grep | Expected |
|-----------------------|------|----------|
| old service-tag import paths, if moved | `rg -n "from \\\"\\.\\./services/.+\\.tag\\\"|from \\\"\\.\\./runtime/.+\\.tag\\\"" cli/src cli/test` | only intended imports/re-exports |

## Discovery (consult log)

- **Coverage baseline (Jun 17, 2026):** `cd cli && npm run test:coverage` passed tests but reported `All files | 97.61 | 99.55`; 1,154 pass / 2 skip / 0 fail. Fresh baseline must be re-run at CHORE(open) because source may move before implementation.
- **Scope split (Jun 17, 2026):** active M92_001 website refresh explicitly says no CLI/API surface. This workstream is separate so the website PR stays reviewable.

## Skill-Driven Review Chain (mandatory)

| When | Skill | What it does | Required output |
|------|-------|--------------|-----------------|
| After implementation, before CHORE(close) | `/write-unit-test` | Audits diff coverage vs this Test Specification. | Clean. Iteration count + final coverage in Discovery. |
| After tests pass, before CHORE(close) | `/review` | Adversarial diff review vs this spec, CLI rules, Effect service layout, retry invariants, and error surfaces. | Clean OR every finding dispositioned. |
| After `gh pr create` | `/review-pr` | Reviews the immutable PR diff for stale generated output, missed tags, and coverage gate drift. | Comments addressed before human review/merge. |

## Verification Evidence

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| CLI coverage | `cd cli && npm run test:coverage` | pending | |
| CLI enforced tests | `cd cli && npm run test` | pending | |
| CLI lint/typecheck | `cd cli && npm run lint && npm run typecheck` | pending | |
| Repo coverage | `make test-coverage-all` | pending | |
| Terminology sweep | `rg -n "zombie|usezombie" cli/src cli/test` | pending | |
| Gitleaks | `gitleaks detect` | pending | |
| Dead code sweep | tag-only audit + import grep | pending | |

## Out of Scope

- M92_001 website copy, footer, End-to-End (E2E) tests, and docs changelog work.
- Package upgrades from `package.json` outdated audit.
- Replacing Commander with an Effect-native CLI runner.
- Any API/schema behavior change.
