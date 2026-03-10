# M6_007: UUIDv7 ID Migration Plan

**Prototype:** v1.0.0
**Milestone:** M6
**Workstream:** 7
**Date:** Mar 10, 2026
**Status:** PENDING
**Priority:** P1 — improve ID ordering and index locality after M5_008 closure
**Batch:** B2 — after M5_008 linkage closure
**Depends on:** M5_008 (immutable compile/activate/run linkage)

---

## 1.0 Scope And Decision

**Status:** PENDING

Define and implement UUIDv7 migration strategy for `run_id`, `profile_version_id`, and `compile_job_id` without breaking existing API/CLI contracts.

**Dimensions:**
- 1.1 PENDING Finalize adopt-now vs phased migration decision with rollback constraints
- 1.2 PENDING Define dual-read/dual-write compatibility contract for existing prefixed IDs
- 1.3 PENDING Define canonical ID formatting contract for API responses and DB persistence
- 1.4 PENDING Define migration ordering and backfill boundaries for pre-existing rows

---

## 2.0 Implementation Contract

**Status:** PENDING

### 2.1 Database and API compatibility

Migration must preserve deterministic behavior during transition and keep run/profile lifecycle queryable.

**Dimensions:**
- 2.1.1 PENDING Add schema/migration support for UUIDv7 IDs with compatibility path
- 2.1.2 PENDING Ensure compile/activate/run linkage remains queryable across mixed ID generations
- 2.1.3 PENDING Add integration tests for old-ID and UUIDv7 interop
- 2.1.4 PENDING Ensure no ambiguous parsing in CLI/API output contracts

---

## 3.0 Acceptance Criteria

**Status:** PENDING

- [ ] 3.1 Existing prefixed-ID rows remain readable and operable after migration
- [ ] 3.2 New rows can be issued as UUIDv7 without breaking API/CLI clients
- [ ] 3.3 Compile/activate/run linkage queries return deterministic chains for mixed IDs
- [ ] 3.4 Migration + rollback runbook and test evidence are captured in docs/evidence

---

## 4.0 Out of Scope

- Broad identifier rewrite outside `run_id`, `profile_version_id`, `compile_job_id`
- Cross-service ID standardization beyond `zombied`
- UI-only identifier presentation changes
