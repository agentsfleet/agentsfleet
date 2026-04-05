# M7_006: DB Schema Migration Gate in deploy-dev Pipeline

**Prototype:** v1.0.0
**Milestone:** M7
**Workstream:** 006
**Date:** Apr 05, 2026
**Status:** IN_PROGRESS
**Priority:** P1 — prevent silent deploy failures from binary/schema mismatch
**Batch:** B2
**Branch:** feat/m7-006-db-schema-gate
**Depends on:** M7_001 (deploy-dev pipeline green), M8_001 (UUID-only schema baseline)

---

## 1.0 Schema Version Contract

**Status:** PENDING

Define one explicit compatibility contract between the running binary and the target database schema so deploy validation is deterministic.

**Dimensions:**
- 1.1 PENDING Define canonical schema version source of truth (migration table or equivalent) and binary expected version source
- 1.2 PENDING Define compatibility rule set: exact match vs forward-compatible range, including handling for fresh environments
- 1.3 PENDING Define failure classes with explicit operator-facing reason codes (`schema_missing`, `version_mismatch`, `db_unreachable`)
- 1.4 PENDING Define a single machine-readable output format for gate checks (status, expected, actual, reason)

---

## 2.0 Runtime Check Primitive

**Status:** PENDING

Implement a deterministic check command/hook that can run in CI and locally before `/readyz` verification.

**Dimensions:**
- 2.1 PENDING Add/extend `zombied doctor` (or dedicated subcommand) to run schema-compat checks against configured DB
- 2.2 PENDING Ensure command exits non-zero on incompatibility and zero on compatibility, with stable output contract
- 2.3 PENDING Ensure check has bounded timeout and clear diagnostics (no silent hangs)
- 2.4 PENDING Add test coverage for match, mismatch, missing migration state, and DB connectivity failure paths

---

## 3.0 deploy-dev.yml Gate Integration

**Status:** PENDING

Insert an explicit migration gate step in CI between image build/push and service readiness verification.

**Dimensions:**
- 3.1 PENDING Update `.github/workflows/deploy-dev.yml` to run schema gate after `build-dev` and before `verify-dev`
- 3.2 PENDING Ensure gate step uses runtime credentials via `op read 'op://...'` patterns and never prints secret values
- 3.3 PENDING Ensure gate failure marks workflow red with actionable summary in logs
- 3.4 PENDING Keep existing `/readyz` check as a separate step so schema failures and service-health failures are distinguishable

---

## 4.0 Operator Guidance and Recovery

**Status:** PENDING

Document how operators recover when gate fails and how to verify resolution.

**Dimensions:**
- 4.1 PENDING Add runbook section describing mismatch triage sequence and migration apply/rollback decision points
- 4.2 PENDING Add explicit local reproduction commands mirroring CI gate behavior
- 4.3 PENDING Add post-fix verification checklist showing schema gate pass + `/readyz` pass

---

## 5.0 Acceptance Criteria

**Status:** PENDING

- [ ] 5.1 `deploy-dev.yml` contains a distinct schema migration gate step between build and verify
- [ ] 5.2 Gate fails fast and explicitly when binary/schema versions are incompatible
- [ ] 5.3 Gate output includes expected version, actual version, and failure reason code
- [ ] 5.4 `/readyz` failures are no longer conflated with schema mismatch failures
- [ ] 5.5 `make lint` and `make test` pass with coverage for gate check logic

---

## 6.0 Out of Scope

- Automatic migration execution in CI
- Production (`deploy-prod`) rollout changes in this workstream
- Online data backfill or zero-downtime migration framework redesign
- Cross-database abstraction beyond current PlanetScale target
