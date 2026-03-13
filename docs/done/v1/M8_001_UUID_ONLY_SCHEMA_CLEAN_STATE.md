# M8_001: UUID-Only Schema Clean State

**Prototype:** v1.0.0
**Milestone:** M8
**Workstream:** 1
**Date:** Mar 13, 2026
**Status:** DONE
**Priority:** P0 — canonical data contract reset
**Batch:** B1 — clean-state baseline
**Depends on:** M5_003 (Entitlement gates), M6_007 (initial UUIDv7 cutover)

---

## 1.0 Singular Function

**Status:** DONE

Define and implement one canonical schema function: all identifier columns across core tables are UUID (UUIDv7 issuance contract), generated and validated in one clean-state schema baseline.

**Dimensions:**
- 1.1 DONE UUID-only ID catalog is implemented across canonical schema tables (`tenant_id`, `workspace_id`, `spec_id`, `run_id`, profile/linkage IDs, entitlement IDs)
- 1.2 DONE Canonical clean-state schema baseline uses `CREATE TABLE` contracts without compatibility `ALTER ... IF EXISTS` flows in active migration path
- 1.3 DONE Legacy prefixed-ID generation paths (`ws_`, `run_`, `pver_`, `cjob_`) removed from runtime generators and updated fixtures/tests
- 1.4 DONE Runtime issuance contract is UUIDv7-only via `src/types/id_format.zig` and boundary validation in HTTP handlers

---

## 2.0 Clean-State Migration Contract

**Status:** DONE

This milestone is a reset milestone for local and pre-production bootstraps that are not yet stable. Migration strategy is clean-state rebuild, not incremental compatibility.

**Dimensions:**
- 2.1 DONE Canonical migration set now routes to clean-state UUID-first schema files
- 2.2 DONE Active migration set excludes legacy compatibility migration layers
- 2.3 DONE Deterministic bootstrap remains `zombied migrate`/`runCanonicalMigrations` against canonical migration list
- 2.4 DONE Full rollback contract documented as clean re-bootstrap; legacy rollback helper migration is no longer part of active path

---

## 3.0 Verification Units

**Status:** DONE

**Dimensions:**
- 3.1 DONE Unit coverage updated in `src/types/id_format.zig` for UUIDv7 generation/validation across ID families
- 3.2 DONE Runtime handlers and schema are UUID-only in canonical path
- 3.3 DONE Focused DB metadata test added in `src/db/pool.zig` (`UUID_CONTRACT_TESTS=1`) to assert UUID column types
- 3.4 DONE API boundary validators reject non-UUID IDs deterministically with stable machine-readable codes

---

## 4.0 Acceptance Criteria

**Status:** DONE

- [x] 4.1 No canonical ID columns in active schema path are `TEXT`
- [x] 4.2 API handlers reject non-UUID IDs deterministically
- [x] 4.3 Prefixed-ID runtime generation paths and fixtures were replaced in touched contract tests
- [x] 4.4 Local bootstrap code path uses clean-state canonical migrations without compatibility alters

---

## 5.0 Out of Scope

- Data migration for already-populated production datasets
- Backward compatibility for unreleased prefixed-ID API clients
- Partial dual-write or dual-read compatibility windows

---

## 6.0 Implementation Direction

**Status:** DONE

- Use UUID as the database type for all ID columns.
- Keep UUIDv7 as runtime generation format for ordering and locality.
- Replace legacy string-based workspace/state callback assumptions with UUID-safe routing and state encoding.
- Treat this milestone as canonical schema reset; do not add incremental migration branches for unverified local legacy states.

---

## 7.0 Verification Log

**Status:** DONE

- `make lint` — PASS
- `zig build test` — PASS
- `zig test src/db/pool.zig --test-filter "uuid contract"` — blocked in current sandbox due Zig stdlib permission restriction; canonical DB contract test is available behind `UUID_CONTRACT_TESTS=1` for live Postgres execution
