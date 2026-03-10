# M5_008: Dynamic Agent Profile End-to-End Workflow

**Prototype:** v1.0.0
**Milestone:** M5
**Workstream:** 8
**Date:** Mar 10, 2026
**Status:** IN_PROGRESS
**Priority:** P0 — close profile workflow gap from config to run execution
**Batch:** B3 — follows M5_002 control-plane baseline
**Depends on:** M4_001 (CLI runtime), M4_003 (profile-driven runtime baseline), M5_002 (harness control plane)

---

## 1.0 Workflow Contract Closure

**Status:** PENDING

Define one complete dynamic agent profile workflow contract that starts at profile source, passes through compile and activation, and is provably enforced during run execution.

**Dimensions:**
- 1.1 PENDING Define canonical sequence: profile source put -> compile -> activate -> run snapshot pinning -> run execution
- 1.2 PENDING Define fail-closed behavior for invalid profile, missing active profile, cross-workspace profile access, and OWASP-aligned prompt-injection/unsafe-instruction detection
- 1.3 PENDING Define immutable audit artifacts for compile/activate/run-profile linkage
- 1.4 PENDING Define deterministic fallback contract (`default-v1`) and when fallback is disallowed

---

## 2.0 CLI/API Operator Path

**Status:** PENDING

Close the operator gap between existing CLI runtime commands and harness profile lifecycle APIs so operators can drive and verify profile changes end-to-end without ad hoc steps.

**Dimensions:**
- 2.1 PENDING Specify required `zombiectl` profile lifecycle commands (or explicit command-group extension) for source put/compile/activate/status
- 2.2 PENDING Define CLI output contract for profile version IDs, active binding, and compile errors (machine-parseable)
- 2.3 PENDING Integration test contract: CLI-triggered profile activation affects subsequent run execution path deterministically
- 2.4 PENDING Contract test: API and CLI expose identical profile identity fields (`profile_id`, `version_id`, `active_at`, `run_snapshot_version`)

---

## 3.0 Architecture And Documentation Consistency

**Status:** IN_PROGRESS

Make architecture docs match shipped profile behavior and remove stale static-stage assumptions from operator-facing references.

**Dimensions:**
- 3.1 DONE Update `docs/ARCHITECTURE.md` execution lifecycle to reference profile-resolved stage topology instead of fixed Echo -> Scout -> Warden sequence
- 3.2 DONE Add dynamic profile control-plane flow to architecture docs (compile/activate/resolve/run)
- 3.3 PENDING Add operator runbook snippet with concrete commands and expected outputs
- 3.4 PENDING Add demo evidence checklist (commands/logs) proving profile switch impacts run behavior

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 A single documented and test-backed workflow exists from profile edit to run execution proof
- [ ] 4.2 Operators can perform profile lifecycle via deterministic CLI/API contracts without undocumented steps
- [ ] 4.3 Architecture documentation matches actual dynamic profile runtime behavior
- [ ] 4.4 Demo evidence is captured for profile switch changing stage execution without worker code changes

---

## 5.0 Out of Scope

- Visual profile editor UI
- Runtime hot-reload of active profile during an in-flight run
- Multi-branch DAG joins and scheduler redesign
- Billing/entitlement expansion beyond profile workflow contract
