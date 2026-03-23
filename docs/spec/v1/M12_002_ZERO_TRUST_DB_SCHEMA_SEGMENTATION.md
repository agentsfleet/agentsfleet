# M12_002: Zero-Trust DB Schema Segmentation

**Prototype:** v1.0.0
**Milestone:** M12
**Workstream:** 002
**Date:** Mar 23, 2026
**Status:** PENDING
**Priority:** P0 — Contain credential blast radius and unblock deterministic deploys
**Batch:** B1 — parallel execution group
**Depends on:** M7_001 (DEV deploy pipeline), existing `vault` schema and migration runner

---

## 1.0 Migration Control Plane Separation

**Status:** PENDING

Separate migration authority from runtime authority so runtime creds cannot execute DDL.

**Dimensions:**
- 1.1 PENDING Introduce dedicated migration credential (`DATABASE_URL_MIGRATOR`) and wire `zombied migrate` to use it.
- 1.2 PENDING Run migrations explicitly in deploy pipeline before health checks; do not rely on runtime startup migration.
- 1.3 PENDING Enforce `MIGRATE_ON_START=0` in runtime paths (API/worker) for deterministic startup behavior.
- 1.4 PENDING Move migration bookkeeping tables to `audit` schema (`audit.schema_migrations`, `audit.schema_migration_failures`).

---

## 2.0 Schema Segmentation Plan

**Status:** PENDING

Fold tables into domain schemas to reduce cross-domain privilege scope.

### 2.1 Target Schema Map

`core`: tenancy + run lifecycle  
`agent`: profile/config/proposal/score domain  
`billing`: metering + entitlement domain  
`vault`: encrypted secret material (existing)  
`audit`: migration + immutable operator audit  
`ops_ro`: read-only masked views for humans/agents

**Dimensions:**
- 2.1.1 PENDING Define canonical table-to-schema mapping and update FK references with explicit schema qualification.
- 2.1.2 PENDING Stage table moves with reversible migration batches (`ALTER TABLE ... SET SCHEMA ...`).
- 2.1.3 PENDING Set explicit `search_path` per role to avoid implicit `public` coupling.
- 2.1.4 PENDING Remove app object creation from `public`; keep `public` non-authoritative.

---

## 3.0 Role and Grant Matrix

**Status:** PENDING

Apply least privilege with separate roles for migrator, runtime, and read planes.

**Dimensions:**
- 3.1 PENDING Create roles: `db_migrator`, `api_runtime`, `worker_runtime`, `ops_readonly_human`, `ops_readonly_agent`.
- 3.2 PENDING Revoke broad defaults (`PUBLIC`) and apply schema-scoped `USAGE` and table-scoped DML grants only.
- 3.3 PENDING Enforce no-DDL runtime policy (runtime roles cannot `CREATE/ALTER/DROP` in any app schema).
- 3.4 PENDING Add deterministic grant verification checks in CI/deploy (`information_schema`/`pg_catalog` assertions).

---

## 4.0 Read-Only Human/Agent Surface

**Status:** PENDING

Expose read models via masked views to avoid direct base-table grants for human/agent operators.

**Dimensions:**
- 4.1 PENDING Create `ops_ro` schema views with minimal columns and secret redaction.
- 4.2 PENDING Route human and autonomous-agent readonly credentials to `ops_ro` only.
- 4.3 PENDING Add audit logging for readonly principal access patterns.
- 4.4 PENDING Add contract checks that no readonly role can query `vault` or mutate `core/agent/billing`.

---

## 5.0 Acceptance Criteria

**Status:** PENDING

- [ ] 5.1 DEV deploy no longer fails with `SQLSTATE 42501` on migration table creation paths.
- [ ] 5.2 Compromised runtime credential simulation cannot run DDL or access `vault` base tables.
- [ ] 5.3 `zombied migrate` succeeds with migrator credential and fails fast with runtime credentials.
- [ ] 5.4 Human/agent readonly credentials can query required operational data through `ops_ro` views only.
- [ ] 5.5 CI includes grant/schema drift checks that fail loudly on privilege regressions.

---

## 6.0 Out of Scope

- Cross-region data replication architecture changes.
- DB engine/provider migration.
- Analytics warehouse redesign.
- Full historical backfill/archival policies beyond schema and grants.
