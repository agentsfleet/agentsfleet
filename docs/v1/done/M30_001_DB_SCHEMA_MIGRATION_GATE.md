# M30_001: DB Schema Migration Gate via Fly release_command

**Prototype:** v1.0.0
**Milestone:** M30
**Workstream:** 001
**Date:** Apr 05, 2026
**Status:** DONE
**Priority:** P1 - prevent silent deploy failures from binary/schema mismatch
**Batch:** B2
**Branch:** feat/m30-001-db-schema-gate
**Depends on:** M7_001 (deploy-dev pipeline green), M8_001 (UUID-only schema baseline)

---

## 1.0 Schema Version Contract

**Status:** DONE

Define one explicit compatibility contract between the running binary and the target database schema so deploy validation is deterministic.

**Dimensions:**
- 1.1 DONE Canonical schema source = `audit.schema_migrations` via `db.inspectMigrationState`; expected binary version = `common.canonicalMigrations()`
- 1.2 DONE Compatibility rule set implemented as strict compatibility: fail on pending migrations, failed migrations, or schema-ahead conditions
- 1.3 DONE Failure classes mapped to explicit `reason_code` strings in schema-gate output (`SCHEMA_COMPATIBLE`, `SCHEMA_BEHIND_BINARY`, `SCHEMA_AHEAD_OF_BINARY`, `SCHEMA_FAILED_MIGRATIONS`)
- 1.4 DONE Machine-readable output implemented through `zombied doctor --schema-gate --format json`

---

## 2.0 Runtime Check Primitive

**Status:** DONE

Implement a deterministic check command/hook that can run in CI and locally before `/readyz` verification.

**Dimensions:**
- 2.1 DONE Extended `zombied doctor` with `--schema-gate` to run schema compatibility checks using migrator DB role
- 2.2 DONE `doctor` returns non-zero when schema gate fails and zero when all checks pass
- 2.3 DONE Diagnostics are explicit and include expected/applied version counts plus reason code
- 2.4 DONE Added targeted tests for doctor argument parsing and schema-gate decision/reason mapping; full test suites pass

---

## 3.0 Deploy Pipeline Alignment (Fly release_command)

**Status:** DONE

Align deploy verification to rely on Fly release command migration execution, not SSH-based schema probes.

**Dimensions:**
- 3.1 DONE Confirmed DEV deploy uses `deploy/fly/zombied-dev/fly.toml` where `[deploy].release_command = "/usr/local/bin/zombied migrate"`
- 3.2 DONE Confirmed PROD release uses `deploy/fly/zombied-prod/fly.toml` where `[deploy].release_command = "/usr/local/bin/zombied migrate"`
- 3.3 DONE Removed explicit SSH schema gate step from `verify-dev` job in `.github/workflows/deploy-dev.yml`
- 3.4 DONE `/readyz` verification remains separate from migration concerns

---

## 4.0 Operator Guidance and Recovery

**Status:** DONE

Document how operators recover when schema mismatch is detected and how to verify resolution.

**Dimensions:**
- 4.1 DONE Added runbook section in `docs/operator/operations/doctor.md` covering mismatch triage and reason-code interpretation
- 4.2 DONE Added explicit local reproduction commands for schema gate mode
- 4.3 DONE Added post-fix verification checklist (`doctor --schema-gate` + `/readyz` confirmation)

---

## 5.0 Acceptance Criteria

**Status:** DONE

- [x] 5.1 Deploy pipelines rely on Fly `release_command` migration execution (DEV and PROD fly configs)
- [x] 5.2 Migration/schema incompatibility failures are surfaced as deploy failures (release command), not hidden behind readiness checks
- [x] 5.3 Schema gate output includes expected version, actual version, and failure reason code
- [x] 5.4 `/readyz` failures are not conflated with schema mismatch failures
- [x] 5.5 `make lint` and `make test` pass with coverage for schema-gate logic

---

## 6.0 Out of Scope

- Automatic rollback of failed migrations
- Online data backfill or zero-downtime migration framework redesign
- Cross-database abstraction beyond current PlanetScale target
