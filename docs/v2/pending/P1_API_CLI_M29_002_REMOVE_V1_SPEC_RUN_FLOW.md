# P1_API_CLI_M29_002: Remove v1 spec/run flow (Zig subcommand + HTTP routes + zombiectl commands)

**Prototype:** v0.18.0
**Milestone:** M29
**Workstream:** 002
**Date:** Apr 18, 2026
**Status:** PENDING
**Priority:** P1 — Pure cleanup. The spec→run flow is no longer the product model; zombies are. Leaving the v1 scaffolding around adds surface area for reviewers to interpret, makes the CLI help confusing, and keeps dead code paths in the build. Not a production blocker because nothing in the v1 flow is relied on today, but worth doing before v1.0.0.
**Batch:** B1 — runs in parallel with M29_001.
**Branch:** feat/m29-remove-v1-spec-run (added when work begins)
**Depends on:** M29_001 ships the replacement docs so users landing on docs.usezombie.com see the zombie-centric model before the CLI subcommands vanish. M28_001 (OpenAPI split) must merge before this workstream touches `public/openapi.json` to avoid bundle conflicts.

---

## Overview

**Goal (testable):** `zombied run` and `zombied spec-validate` are no longer valid subcommands; `zombiectl run`, `runs`, `run-preview`, `run-watch`, `run-interrupt`, `spec init` are no longer valid commands; `POST /v1/workspaces/{ws}/spec/template` and `.../spec/preview` return 404 from the router (matcher deleted). Orphan sweep on `spec_validator`, `cmd_run`, `invokeSpecTemplate`, `invokeSpecPreview`, `run_preview`, `spec_init` returns zero non-archaeology hits across Zig + JS + tests.

**Problem:** Three observable symptoms: (1) `zombied --help` still advertises `run` as a subcommand whose implementation (`cmd/run.zig`) reads a spec file and invokes `spec_validator.zig` — a validator for a spec format that no longer exists as a product primitive. (2) `zombiectl --help` still lists `run`, `runs`, `run preview`, `run watch`, `run interrupt`, `spec init` among its commands; running any of them hits `/spec/template` or `/spec/preview` or returns-list endpoints that are slated for removal. (3) `src/http/handlers/agent_relay.zig` contains `SPEC_TEMPLATE_SYSTEM_PROMPT` and `SPEC_PREVIEW_SYSTEM_PROMPT` constants wired into two router entries; the handler itself (a generic LLM relay primitive) stays, but the spec-specific prompts and their routes go.

**Solution summary:** Delete the Zig subcommand (`src/cmd/run.zig`, `src/cmd/run_watch.zig`, both tests, `src/cmd/spec_validator.zig` + test) and their dispatcher in `src/main.zig`. Remove the `Subcommand.run` variant from `src/cli/commands.zig`. Delete the `spec_template` and `spec_preview` router variants, their invokers, and the two system-prompt constants in `agent_relay.zig` — the handler stays as a generic relay primitive per user direction. Delete the entire zombiectl command family (`run_preview.js`, `run_preview_walk.js`, `run_watch.js`, `run_interrupt.js`, `runs.js`, `spec_init.js`) and their tests. Unwire them from `zombiectl/src/cli.js` and clean `zombiectl/src/commands/core.js` + `zombiectl/test/json-contract.test.js`. Remove `public/openapi.json` entries for `/v1/workspaces/{ws}/spec/template` and `.../spec/preview` after the sibling M28_001 OPENAPI_SPLIT_AND_SVIX_DOCS branch merges to main. Orphan-sweep every removed symbol across the tree. No schema changes — v1 `core.specs` / `core.runs` tables were torn down earlier; only two archaeology comments remain and those stay as archaeology.

---

## Files Changed (blast radius)

| File | Action | Why |
|------|--------|-----|
| `src/cmd/run.zig` | DELETE | `zombied run <spec>` subcommand — v1 flow |
| `src/cmd/run_watch.zig` | DELETE | `--watch` flag helper for `zombied run` |
| `src/cmd/run_watch_test.zig` | DELETE | tests the helper |
| `src/cmd/spec_validator.zig` | DELETE | pre-flight validation for v1 spec YAML |
| `src/cmd/spec_validator_test.zig` | DELETE | tests the validator |
| `src/cli/commands.zig` | MODIFY | drop `Subcommand.run` enum variant + its two tests |
| `src/main.zig` | MODIFY | drop `cmd_run` import, `.run =>` dispatcher arm, `@import("cmd/run_watch_test.zig")`, and the help-text comment describing `run` |
| `src/http/handlers/agent_relay.zig` | MODIFY | delete `SPEC_TEMPLATE_SYSTEM_PROMPT`, `SPEC_PREVIEW_SYSTEM_PROMPT`, and the two handler functions that use them; keep the generic relay primitive |
| `src/http/router.zig` | MODIFY | delete `.spec_template` + `.spec_preview` enum variants and their `match()` arms |
| `src/http/route_table.zig` | MODIFY | delete the two `specFor` arms |
| `src/http/route_table_invoke.zig` | MODIFY | delete `invokeSpecTemplate` + `invokeSpecPreview` |
| `src/http/router_test.zig` | MODIFY | delete the two route matcher tests |
| `public/openapi.json` | MODIFY | delete `/v1/workspaces/{ws}/spec/template` + `.../spec/preview` path entries (after M28_001 merges to main) |
| `zombiectl/src/commands/run_preview.js` | DELETE | v1 preview command |
| `zombiectl/src/commands/run_preview_walk.js` | DELETE | walker helper for run_preview |
| `zombiectl/src/commands/run_watch.js` | DELETE | v1 run watch |
| `zombiectl/src/commands/run_interrupt.js` | DELETE | v1 run interrupt |
| `zombiectl/src/commands/runs.js` | DELETE | v1 runs list/cancel/replay |
| `zombiectl/src/commands/spec_init.js` | DELETE | v1 spec scaffold |
| `zombiectl/test/run-preview.{unit,integration,edge,security}.test.js` | DELETE | tests for run_preview |
| `zombiectl/test/run-watch.unit.test.js` | DELETE | tests for run_watch |
| `zombiectl/test/runs-{replay,interrupt,cancel}.unit.test.js` | DELETE | tests for runs |
| `zombiectl/test/spec-init.{unit,integration,edge,security}.test.js` | DELETE | tests for spec_init |
| `zombiectl/src/cli.js` | MODIFY | unwire removed commands (remove imports + dispatch arms) |
| `zombiectl/src/commands/core.js` | MODIFY | remove run/spec references in core dispatch |
| `zombiectl/test/json-contract.test.js` | MODIFY | drop JSON-shape contracts for removed commands |
| `make/quality.mk` | MODIFY | remove the spec-lint target if present |

## Applicable Rules

- **RULE ORP** — cross-layer orphan sweep across Zig + JS + OpenAPI + comments. Every removed symbol must grep to 0 hits in non-historical files.
- **RULE CHR** — changelog updated with a user-facing announcement of the removal (see CHORE(close)).
- **RULE FLL** — no new file-length violations introduced by the edits; reviewers verify touched files stay ≤ 350 lines.
- **RULE XCC** — cross-compile `x86_64-linux` + `aarch64-linux` after the Zig deletes.
- Standard set otherwise.

---

## Sections (implementation slices)

### §1 — Remove `zombied run` subcommand + `spec_validator` module

**Status:** PENDING

Delete the four files in `src/cmd/` and drop the enum variant + dispatcher. This makes `zombied --help` stop advertising `run` and removes the `spec_validator` module entirely.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 1.1 | PENDING | `src/cli/commands.zig:Subcommand` | enum definition post-edit | no `run` variant; `parseSubcommandName("run")` defaults to `.serve` | unit |
| 1.2 | PENDING | `src/main.zig` | post-edit | no `cmd_run` import; no `.run =>` arm; no `@import("cmd/run_watch_test.zig")` | grep |
| 1.3 | PENDING | `src/cmd/` directory | listing post-edit | `run.zig`, `run_watch.zig`, `run_watch_test.zig`, `spec_validator.zig`, `spec_validator_test.zig` all absent | contract |
| 1.4 | PENDING | `zig build` | post-edit | compiles; `zombied --help` output does not list `run` | integration |

### §2 — Remove `spec_template` / `spec_preview` HTTP routes

**Status:** PENDING

Delete the two router enum variants, their registration in `route_table.zig`, their invoker functions in `route_table_invoke.zig`, and the two system-prompt constants + handler functions in `agent_relay.zig`. The generic relay primitive (the rest of `agent_relay.zig`) stays.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 2.1 | PENDING | `src/http/router.zig:Route` | enum definition post-edit | no `.spec_template` or `.spec_preview` variants | unit |
| 2.2 | PENDING | `src/http/router.zig:match` | input `/v1/workspaces/ws1/spec/template` | returns `null` | unit |
| 2.3 | PENDING | `src/http/handlers/agent_relay.zig` | post-edit | no `SPEC_TEMPLATE_SYSTEM_PROMPT` or `SPEC_PREVIEW_SYSTEM_PROMPT` identifiers; file otherwise intact (relay primitive stays) | grep |
| 2.4 | PENDING | live HTTP server | `POST /v1/workspaces/ws1/spec/template` | 404 (pre-v2.0 teardown — no 410 stub) | integration |

### §3 — Remove zombiectl run/spec commands + tests

**Status:** PENDING

Delete six command files and every test file that targets them. Unwire them from `cli.js`, clean `core.js`, and prune `json-contract.test.js` entries.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 3.1 | PENDING | `zombiectl/src/commands/` listing | post-edit | `run_preview.js`, `run_preview_walk.js`, `run_watch.js`, `run_interrupt.js`, `runs.js`, `spec_init.js` all absent | contract |
| 3.2 | PENDING | `zombiectl/test/` listing | post-edit | every matching test file absent (see Dead Code Sweep) | contract |
| 3.3 | PENDING | `zombiectl run` invocation | shell exit | non-zero exit with `unknown command: run` | integration |
| 3.4 | PENDING | `npm test` in `zombiectl/` | post-edit | all remaining tests green | integration |

### §4 — OpenAPI + make/quality.mk + archaeology comments

**Status:** PENDING

Delete the two `/spec/*` path entries from `public/openapi.json` (only after M28_001 merges to main; verify with `git log origin/main` before touching). Remove any spec-lint target from `make/quality.mk`. Leave the two archaeology comments in `schema/004_vault_schema.sql:40` and `schema/020_...sql:1,77` — they are pre-v1 history and the terminology rule exempts historical artifacts.

**Dimensions (test blueprints):**

| Dim | Status | Target | Input | Expected | Test type |
|-----|--------|--------|-------|----------|-----------|
| 4.1 | PENDING | `public/openapi.json` | `jq '.paths | keys'` post-edit | no `/v1/workspaces/{workspace_id}/spec/template` or `.../spec/preview` entries | unit (jq) |
| 4.2 | PENDING | `make/quality.mk` | post-edit | no `spec-*` make target | grep |
| 4.3 | PENDING | `make check-openapi-errors` | post-edit | passes (OpenAPI matches router) | integration |
| 4.4 | PENDING | orphan sweep | see Dead Code Sweep below | 0 non-historical hits | contract |

---

## Interfaces

**Status:** PENDING

N/A — pure removal workstream. No new public functions, endpoints, or data shapes.

### Error Contracts

| Error condition | Behavior | Caller sees |
|----------------|----------|-------------|
| Client invokes `zombied run` | binary prints usage and exits 1 | `unknown subcommand: run` |
| Client invokes `zombiectl run ...` or `zombiectl spec init` | Node CLI prints usage and exits 2 | `unknown command: run` / `unknown command: spec` |
| Client POSTs to `/v1/workspaces/{ws}/spec/template` | router returns 404 | RFC 7807 problem+json with code `ERR_NOT_FOUND` |

---

## Failure Modes

**Status:** PENDING

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| Automated client still calls `/spec/template` | cached client binary | router 404 | 404 — client breaks; expected post-v1 removal |
| Stale `zombied run --watch` daemon on an operator's box | process started pre-removal | existing process keeps running but cannot reconnect; on restart `--help` no longer lists `run` | process eventually killed |
| Orphan `_ = @import("cmd/run_watch_test.zig")` remains in main.zig | incomplete edit | `zig build` fails with "file not found" | compile error; caught before commit |
| Orphan reference in zombiectl `core.js` | incomplete edit | `node zombiectl.js` throws on startup | CLI dies immediately |
| `agent_relay.zig` is accidentally deleted | over-eager delete | build fails; relay primitive gone | compile error; caught before commit |
| OpenAPI edit races with M28_001 merge | author forgets the sequencing | bundle conflict on rebase | `git rebase` conflict in `public/openapi.json` |

**Platform constraints:**
- Pre-v2.0 teardown: removed endpoints return 404, not 410 (per existing convention — no ceremony for pre-v1 surface).
- The v1 `core.specs` / `core.runs` tables no longer exist in the schema; no DB migration is needed.

---

## Implementation Constraints (Enforceable)

**Status:** PENDING

| Constraint | How to verify |
|-----------|---------------|
| Zero hits for `spec_validator`, `cmd_run`, `run_preview`, `run_watch`, `spec_init`, `SPEC_TEMPLATE_SYSTEM_PROMPT`, `SPEC_PREVIEW_SYSTEM_PROMPT`, `invokeSpecTemplate`, `invokeSpecPreview`, `.spec_template`, `.spec_preview`, `Subcommand.run` across `src/` + `zombiectl/src/` + `zombiectl/test/` | grep pipeline (see Eval Commands) |
| `agent_relay.zig` still compiles and the generic LLM-relay handler still serves at least one route | `zig build` + `grep "fn inner" src/http/handlers/agent_relay.zig` returns the retained handler(s) |
| `zombiectl` still launches without errors | `node zombiectl/src/cli.js --help` exits 0 |
| Pre-v2.0 teardown of removed endpoints — 404 not 410 | integration test hitting the dead endpoint asserts 404 |
| Cross-compile green | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |
| `make lint` + `make check-openapi-errors` + `make check-pg-drain` all green | make run |
| Two archaeology comments (`schema/004_vault_schema.sql:40`, `schema/020_...sql:1,77`) survive — they are history, not active surface | grep confirms they remain |

---

## Invariants (Hard Guardrails)

**Status:** PENDING

| # | Invariant | Enforcement mechanism |
|---|-----------|----------------------|
| 1 | `Subcommand` enum has no `run` variant | Zig `std.meta.fields(Subcommand)` comptime assertion + a negative unit test |
| 2 | `Route` enum has no `spec_template` or `spec_preview` variant | Zig comptime field-name assertion + router_test ensures `match("/v1/.../spec/template")` returns `null` |
| 3 | `agent_relay.zig` contains no identifier named `SPEC_TEMPLATE_SYSTEM_PROMPT` or `SPEC_PREVIEW_SYSTEM_PROMPT` | comptime `@hasDecl` assertion is FALSE (fails build if anyone reintroduces it) |

---

## Test Specification

**Status:** PENDING

### Unit Tests

| Test name | Dim | Target | Input | Expected |
|-----------|-----|--------|-------|----------|
| Subcommand enum has no run variant | 1.1 | `src/cli/commands.zig:Subcommand` | comptime | field count matches the expected set (serve, worker, doctor, migrate, reconcile) — `run` absent |
| parseSubcommandName("run") returns .serve | 1.1 | `parseSubcommandName` | `"run"` | `.serve` (unknown → default) |
| Router.match rejects /v1/.../spec/template | 2.2 | `src/http/router.zig:match` | `"/v1/workspaces/ws1/spec/template"` | `null` |
| Router.match rejects /v1/.../spec/preview | 2.2 | `src/http/router.zig:match` | `"/v1/workspaces/ws1/spec/preview"` | `null` |
| agent_relay has no SPEC_*_SYSTEM_PROMPT | invariant #3 | `src/http/handlers/agent_relay.zig` | `@hasDecl` | FALSE |

### Integration Tests

| Test name | Dim | Infra needed | Input | Expected |
|-----------|-----|-------------|-------|----------|
| zombied --help omits run | 1.4 | built binary | `zombied --help` | stdout does not contain the word "run" as a subcommand entry |
| POST /v1/workspaces/ws/spec/template returns 404 | 2.4 | live server | HTTP POST | status 404, RFC 7807 body |
| zombiectl run exits non-zero | 3.3 | node runtime | `node zombiectl/src/cli.js run` | exit ≥ 1, stderr mentions "unknown command" |
| OpenAPI matches router | 4.3 | `make check-openapi-errors` | diff | no `/spec/*` paths |

### Negative Tests (error paths that MUST fail)

| Test name | Dim | Input | Expected error |
|-----------|-----|-------|---------------|
| reintroducing `.spec_template` variant fails build | invariant #2 | local edit adding the variant | `zig build` fails with comptime assertion |
| reintroducing `SPEC_TEMPLATE_SYSTEM_PROMPT` constant fails build | invariant #3 | local edit adding the constant | `zig build` fails with comptime assertion |

### Edge Case Tests (boundary values)

| Test name | Dim | Input | Expected |
|-----------|-----|-------|----------|
| path `/v1/workspaces/ws1/spec` (no trailing segment) | 2.2 | router input | `null` (no accidental partial match) |
| path `/v1/workspaces/ws1/spec/` | 2.2 | router input | `null` |
| zombiectl argv containing `spec init --force` | 3.3 | node CLI | `unknown command: spec` exit 2 |

### Regression Tests (pre-existing behavior that MUST NOT change)

| Test name | What it guards | File |
|-----------|---------------|------|
| existing `zombied serve`, `worker`, `reconcile`, `migrate`, `doctor` subcommands still dispatch | other Subcommand variants untouched | `src/cli/commands.zig` |
| agent_relay generic relay handler still serves its non-spec route(s) | handler primitive preserved | `src/http/handlers/agent_relay.zig` |
| all non-run/spec zombiectl commands still work | unrelated command families intact | `zombiectl/test/*.test.js` (other than the deleted ones) |
| existing /v1/workspaces/{ws}/agent-keys and all other workspace routes still match | router untouched outside of spec routes | `src/http/router_test.zig` |

### Leak Detection Tests

N/A — removal workstream. The deletes shrink the surface; no new allocators or owners introduced.

### Spec-Claim Tracing

| Spec claim | Test that proves it | Test type |
|-----------|---------------------|-----------|
| "`zombied run` no longer valid" | integration: `zombied --help` + unit: parseSubcommandName | unit + integration |
| "`POST /spec/template` returns 404" | integration: live server | integration |
| "orphan sweep returns 0" | eval command E4 | contract |
| "agent_relay primitive preserved" | regression test on relay handler | regression |

---

## Execution Plan (Ordered)

**Status:** PENDING

| Step | Action | Verify (must pass before next step) |
|------|--------|--------------------------------------|
| 1 | Delete `src/cmd/spec_validator.zig` + `src/cmd/spec_validator_test.zig` | `zig build` compiles (nothing depends on spec_validator outside of run.zig) |
| 2 | Delete `src/cmd/run.zig`, `run_watch.zig`, `run_watch_test.zig`; remove imports + dispatcher in `src/main.zig`; drop `Subcommand.run` + its two tests in `src/cli/commands.zig` | `zig build && zig build test` |
| 3 | Remove `.spec_template` + `.spec_preview` enum variants from `src/http/router.zig`; delete their `match()` arms | `zig build` |
| 4 | Remove their entries from `src/http/route_table.zig` and `src/http/route_table_invoke.zig`; drop `invokeSpecTemplate` + `invokeSpecPreview` | `zig build && zig build test` |
| 5 | Edit `src/http/handlers/agent_relay.zig`: delete `SPEC_TEMPLATE_SYSTEM_PROMPT`, `SPEC_PREVIEW_SYSTEM_PROMPT`, and the two handler functions that used them; keep the relay primitive | `zig build && zig build test` (relay handler's other routes still compile) |
| 6 | Remove the two router tests in `src/http/router_test.zig` | `zig build test` |
| 7 | Delete zombiectl command files (6) and test files (11); unwire them from `zombiectl/src/cli.js`; clean `core.js` + `json-contract.test.js` | `npm --prefix zombiectl test` |
| 8 | After M28_001 has merged to main (verify with `git log origin/main`), rebase and remove `/v1/workspaces/{ws}/spec/template` + `.../spec/preview` from `public/openapi.json` | `make check-openapi-errors` green |
| 9 | Remove any `spec-*` target from `make/quality.mk` | `make lint` green |
| 10 | Orphan sweep | see Dead Code Sweep — zero non-historical hits |
| 11 | Cross-compile + lint + gitleaks | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux && make lint && gitleaks detect` |

---

## Acceptance Criteria

**Status:** PENDING

- [ ] `zombied --help` does not list `run` — verify: `zombied --help | grep -w run` returns nothing
- [ ] `zombiectl --help` does not list `run`, `runs`, `run-preview`, `run-watch`, `run-interrupt`, `spec` — verify: grep
- [ ] `POST /v1/workspaces/ws/spec/template` returns 404 — verify: integration test
- [ ] Orphan sweep returns zero hits for every symbol listed in Dead Code Sweep — verify: grep pipeline
- [ ] `zig build && zig build test` green — verify: CI
- [ ] `npm --prefix zombiectl test` green — verify: CI
- [ ] `make lint` + `make check-openapi-errors` + `make check-pg-drain` all green — verify: make run
- [ ] Cross-compile passes — verify: `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux`
- [ ] `agent_relay.zig` still compiles and its remaining route(s) still serve requests — verify: `zig build` + retained handler grep
- [ ] Changelog entry under `<Update>` tagged `Breaking` — verify: diff inspection

---

## Eval Commands (Post-Implementation Verification)

**Status:** PENDING

```bash
# E1: Zig build
zig build 2>&1 | tail -5; echo "build=$?"

# E2: Tests
zig build test 2>&1 | tail -5; echo "test=$?"

# E3: zombiectl tests
npm --prefix zombiectl test 2>&1 | tail -10; echo "js=$?"

# E4: Orphan sweep — zero hits across Zig + JS (not counting historical schema comments)
SYMS='spec_validator|cmd_run|SPEC_TEMPLATE_SYSTEM_PROMPT|SPEC_PREVIEW_SYSTEM_PROMPT|invokeSpecTemplate|invokeSpecPreview|\.spec_template|\.spec_preview|run_preview|run_watch|run_interrupt|spec_init'
rg -nE "$SYMS" src/ zombiectl/src/ zombiectl/test/ | grep -v 'schema/004_vault_schema.sql' | grep -v 'schema/020_' | head -20
echo "E4: orphan sweep (empty = pass)"

# E5: Live server — deleted endpoints are 404
curl -s -o /dev/null -w '%{http_code}' -X POST http://localhost:8080/v1/workspaces/ws1/spec/template; echo
curl -s -o /dev/null -w '%{http_code}' -X POST http://localhost:8080/v1/workspaces/ws1/spec/preview; echo

# E6: OpenAPI alignment
make check-openapi-errors 2>&1 | tail -5

# E7: Lint + gitleaks
make lint 2>&1 | tail -5
gitleaks detect 2>&1 | tail -3

# E8: Cross-compile
zig build -Dtarget=x86_64-linux 2>&1 | tail -3; echo "x86=$?"
zig build -Dtarget=aarch64-linux 2>&1 | tail -3; echo "arm=$?"

# E9: 350-line gate
git diff --name-only origin/main | grep -v -E '\.md$|^vendor/|_test\.|\.test\.|\.spec\.|/tests?/' \
  | xargs -I{} sh -c 'wc -l "{}"' | awk '$1 > 350 { print "OVER: " $2 ": " $1 }'
```

---

## Dead Code Sweep

**Status:** PENDING

**1. Orphaned files — must be deleted from disk and git.**

| File to delete | Verify deleted |
|---------------|----------------|
| `src/cmd/run.zig` | `test ! -f src/cmd/run.zig` |
| `src/cmd/run_watch.zig` | `test ! -f src/cmd/run_watch.zig` |
| `src/cmd/run_watch_test.zig` | `test ! -f src/cmd/run_watch_test.zig` |
| `src/cmd/spec_validator.zig` | `test ! -f src/cmd/spec_validator.zig` |
| `src/cmd/spec_validator_test.zig` | `test ! -f src/cmd/spec_validator_test.zig` |
| `zombiectl/src/commands/run_preview.js` | `test ! -f zombiectl/src/commands/run_preview.js` |
| `zombiectl/src/commands/run_preview_walk.js` | `test ! -f zombiectl/src/commands/run_preview_walk.js` |
| `zombiectl/src/commands/run_watch.js` | `test ! -f zombiectl/src/commands/run_watch.js` |
| `zombiectl/src/commands/run_interrupt.js` | `test ! -f zombiectl/src/commands/run_interrupt.js` |
| `zombiectl/src/commands/runs.js` | `test ! -f zombiectl/src/commands/runs.js` |
| `zombiectl/src/commands/spec_init.js` | `test ! -f zombiectl/src/commands/spec_init.js` |
| `zombiectl/test/run-preview.{unit,integration,edge,security}.test.js` | each `test ! -f` |
| `zombiectl/test/run-watch.unit.test.js` | `test ! -f` |
| `zombiectl/test/runs-{replay,interrupt,cancel}.unit.test.js` | each `test ! -f` |
| `zombiectl/test/spec-init.{unit,integration,edge,security}.test.js` | each `test ! -f` |

**2. Orphaned references — zero remaining imports or uses.**

| Deleted symbol or import | Grep command | Expected |
|-------------------------|--------------|----------|
| `spec_validator` | `rg -n "spec_validator" src/ zombiectl/` | 0 matches |
| `cmd_run` / `cmd/run` import | `rg -n 'cmd/run\|cmd_run' src/` | 0 matches |
| `Subcommand.run` / `.run =>` | `rg -nE '\.run\s*=>' src/` | 0 matches |
| `SPEC_TEMPLATE_SYSTEM_PROMPT` | `rg -n "SPEC_TEMPLATE_SYSTEM_PROMPT" src/` | 0 matches |
| `SPEC_PREVIEW_SYSTEM_PROMPT` | `rg -n "SPEC_PREVIEW_SYSTEM_PROMPT" src/` | 0 matches |
| `invokeSpecTemplate` / `invokeSpecPreview` | `rg -nE "invokeSpec(Template\|Preview)" src/` | 0 matches |
| `.spec_template` / `.spec_preview` | `rg -nE '\.(spec_template\|spec_preview)\b' src/` | 0 matches |
| `run_preview`, `run_watch`, `run_interrupt`, `spec_init` (JS) | `rg -nE '(run_preview\|run_watch\|run_interrupt\|spec_init)' zombiectl/src/ zombiectl/test/` | 0 matches |
| `/spec/template` / `/spec/preview` (OpenAPI) | `jq '.paths \| keys[]' public/openapi.json \| grep -E '/spec/'` | 0 matches |

**3. main.zig test discovery — update imports.**

Remove `_ = @import("cmd/run_watch_test.zig");` from `src/main.zig`. No additions.

**Exemptions (archaeology — DO keep):**
- `schema/004_vault_schema.sql:40` — comment referencing pre-v1 `core.specs, core.runs, core.run_transitions, core.artifacts`. Pre-v1 history, stays.
- `schema/020_agent_failure_analysis_and_context_injection.sql:1,77` — comments noting v1 tables that were torn down. Stays.
- `docs/v2/done/**` — every closed spec referencing M9_001, M10_001, M16_002, etc. Immutable by convention.
- `docs/nostromo/LOG_*` and `HANDOFF_*` — immutable logs.

---

## Verification Evidence

**Status:** PENDING

| Check | Command | Result | Pass? |
|-------|---------|--------|-------|
| Unit + integration tests | `zig build test` | | |
| Zombiectl tests | `npm --prefix zombiectl test` | | |
| Orphan sweep | E4 from Eval Commands | | |
| Live endpoint 404 | E5 from Eval Commands | | |
| OpenAPI alignment | `make check-openapi-errors` | | |
| Lint | `make lint` | | |
| Gitleaks | `gitleaks detect` | | |
| Cross-compile | `zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` | | |
| 350L gate | E9 from Eval Commands | | |

---

## Out of Scope

- Adding a `zombiectl webhook` or `zombiectl vault` command family — those are replacement commands for a future CLI milestone.
- Rewriting `agent_relay.zig` into a purely generic LLM relay with a pluggable prompt system — keep the handler as-is; remove only the two spec-specific prompts.
- Touching `/Users/kishore/Projects/docs` — the doc rewrite is the companion workstream M29_001.
- Deleting or modifying archaeology comments in `schema/004_vault_schema.sql:40` and `schema/020_...sql:1,77` — historical, exempt.
- Producing any 410 Gone stubs for the deleted endpoints — pre-v2.0 convention is 404 (per repo memory: `feedback_pre_v2_api_drift.md`).
- Migrating any existing `core.specs` / `core.runs` rows — these tables no longer exist in the schema.
- Changelog authorship — happens in CHORE(close) when the workstream activates, not here.
