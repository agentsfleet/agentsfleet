# M5_008: Dynamic Agent Profile End-to-End Workflow

**Prototype:** v1.0.0
**Milestone:** M5
**Workstream:** 8
**Date:** Mar 10, 2026
**Status:** DONE
**Priority:** P0 — close profile workflow gap from config to run execution
**Batch:** B3 — follows M5_002 control-plane baseline
**Depends on:** M4_001 (CLI runtime), M4_003 (profile-driven runtime baseline), M5_002 (harness control plane)

---

## 1.0 Workflow Contract Closure

**Status:** DONE

Define one complete dynamic agent profile workflow contract that starts at profile source, passes through compile and activation, and is provably enforced during run execution.

**Dimensions:**
- 1.1 DONE Define canonical sequence: profile source put -> compile -> activate -> run snapshot pinning -> run execution
- 1.2 DONE Define fail-closed behavior for invalid profile, missing active profile, cross-workspace profile access, and OWASP-aligned prompt-injection/unsafe-instruction detection
- 1.3 DONE Define immutable audit artifacts for compile/activate/run-profile linkage
- 1.4 DONE Define deterministic fallback contract (`default-v1`) and when fallback is disallowed

---

## 2.0 CLI/API Operator Path

**Status:** DONE

Close the operator gap between existing CLI runtime commands and harness profile lifecycle APIs so operators can drive and verify profile changes end-to-end without ad hoc steps.

**Dimensions:**
- 2.1 DONE Specify required `zombiectl` profile lifecycle commands (or explicit command-group extension) for source put/compile/activate/status
- 2.2 DONE Define CLI output contract for profile version IDs, active binding, and compile errors (machine-parseable)
- 2.3 DONE Integration test contract: CLI-triggered profile activation affects subsequent run execution path deterministically
- 2.4 DONE Contract test: API and CLI expose identical profile identity fields (`profile_id`, `version_id`, `active_at`, `run_snapshot_version`)

---

## 3.0 Architecture And Documentation Consistency

**Status:** DONE

Make architecture docs match shipped profile behavior and remove stale static-stage assumptions from operator-facing references.

**Dimensions:**
- 3.1 DONE Update `docs/ARCHITECTURE.md` execution lifecycle to reference profile-resolved stage topology instead of fixed Echo -> Scout -> Warden sequence
- 3.2 DONE Add dynamic profile control-plane flow to architecture docs (compile/activate/resolve/run)
- 3.3 DONE Add operator runbook snippet with concrete commands and expected outputs
- 3.4 DONE Add demo evidence checklist (commands/logs) proving profile switch impacts run behavior

---

## 4.0 Acceptance Criteria

**Status:** DONE

- [x] 4.1 A single documented and test-backed workflow exists from profile edit to run execution proof
- [x] 4.2 Operators can perform profile lifecycle via deterministic CLI/API contracts without undocumented steps
- [x] 4.3 Architecture documentation matches actual dynamic profile runtime behavior
- [x] 4.4 Demo evidence is captured for profile switch changing stage execution without worker code changes

---

## 5.0 Out of Scope

- Visual profile editor UI
- Runtime hot-reload of active profile during an in-flight run
- Multi-branch DAG joins and scheduler redesign
- Billing/entitlement expansion beyond profile workflow contract

---

## 6.0 Evidence And Next Steps (No Mission Control UI)

**Status:** DONE

This workstream remains CLI/API-first. Mission Control UI work is deferred to a future milestone and is not required for M5_008 closure.

Completed evidence anchors:
- API harness lifecycle endpoints are shipped in `src/http/server.zig` + `src/http/handler.zig` + `src/http/handlers/harness_control_plane/*.zig`.
- CLI harness command group is shipped in `zombiectl/src/commands/harness.js` and wired in `zombiectl/src/cli.js`.
- CLI unit coverage exists for harness source/compile payload contracts in `zombiectl/test/harness-command.unit.test.js`, `zombiectl/test/harness-source-put.test.js`, and `zombiectl/test/harness-compile.test.js`.
- Profile compile fail-closed checks (unsafe/prompt-injection/secret fields) exist in `src/harness/control_plane.zig` tests.
- Worker integration coverage proves active-profile switch changes resolved execution topology in `src/pipeline/worker_profile_tests.zig`.
- Run snapshot linkage is persisted and exposed via `schema/008_run_snapshot_version.sql`, `src/http/handlers/runs/*.zig`, and surfaced in CLI run status in `zombiectl/src/cli.js`.
- Immutable compile/activate/run linkage artifacts are synchronously persisted in `profile_linkage_audit_artifacts` via `schema/009_profile_linkage_audit.sql` and handler modules `src/http/handlers/harness_control_plane/compile.zig`, `src/http/handlers/harness_control_plane/activate.zig`, and `src/http/handlers/runs/start.zig`.
- Run linkage is queryable from `GET /v1/runs/{run_id}` through `src/audit/profile_linkage.zig` and `src/http/handlers/runs/get.zig`.
- CLI lifecycle integration and parity contract tests are shipped in `zombiectl/test/harness-lifecycle.integration.test.js`.
- Control-plane identity linkage contract is covered in `src/http/handlers/harness_control_plane/tests.zig` integration test `integration: activate/getActive profile identity contract includes snapshot linkage fields`.
- Immutable artifact and queryability coverage is shipped in `src/audit/profile_linkage.zig` integration tests:
  - `integration: linkage chain is queryable for run`
  - `integration: linkage artifacts are immutable and reject updates`
- Hardening/closure coverage added for immutable linkage reliability:
  - `integration: compileProfile is atomic and rolls back on linkage persist failure`
  - `integration: activateProfile is atomic and rolls back on linkage persist failure`
  - `integration: get-run response payload includes profile_linkage chain contract`
  - `integration: run linkage insert fails closed when snapshot profile version does not exist`
  - `integration: compileProfile rejects cross-workspace profile version selector`
  - `integration: activateProfile rejects profile versions from another workspace`

UUID strategy decision for this workstream:
- `run_id`, `profile_version_id`, and `compile_job_id` remain existing prefixed IDs for M5_008 closure.
- UUIDv7 migration is deferred to follow-up workstream `docs/spec/v1/M6_007_UUIDV7_ID_MIGRATION_PLAN.md`.

Evidence tracker:
- `docs/evidence/M5_008_PROFILE_SWITCH_DEMO.md`.
