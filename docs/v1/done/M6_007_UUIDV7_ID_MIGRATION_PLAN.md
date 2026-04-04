# M6_007: UUIDv7 ID Migration Plan

**Prototype:** v1.0.0
**Milestone:** M6
**Workstream:** 7
**Date:** Mar 10, 2026
**Status:** DONE
**Priority:** P1 — improve ID ordering and index locality after M5_008 closure
**Batch:** B2 — after M5_008 linkage closure
**Depends on:** M5_008 (immutable compile/activate/run linkage)
**Error Docs Base:** `https://docs.e2enetworks.com/error-codes#`
**Agent Link Rule:** `resolved_link = ERROR_DOCS_BASE + ERROR_CODE`

---

## 1.0 Scope And Decision

**Status:** DONE

Define and implement UUIDv7 migration strategy for `run_id`, `profile_version_id`, and `compile_job_id`.  
Decision finalized with owner direction (`Mar 12, 2026`): pre-production hard cutover to UUIDv7-typed DB columns; remove legacy prefixed ID runtime path.

**Dimensions:**
- 1.1 DONE Finalize migration decision with rollback constraints (`UZ-UUIDV7-001`)
- 1.2 DONE Replace dual-format contract with UUIDv7-only cutover contract (`UZ-UUIDV7-002`)
- 1.3 DONE Define canonical ID formatting contract for API responses and DB persistence (`UZ-UUIDV7-003`)
- 1.4 DONE Define migration ordering and fail-fast boundaries for pre-existing rows (`UZ-UUIDV7-004`)

---

## 2.0 Implementation Contract

**Status:** DONE

### 2.1 Database and API compatibility

Migration preserves deterministic behavior and keeps run/profile lifecycle queryable after UUID type conversion.

**Dimensions:**
- 2.1.1 DONE Add schema migration support for UUIDv7 typed columns (`UZ-UUIDV7-005`)
- 2.1.2 DONE Ensure compile/activate/run linkage remains queryable with UUIDv7 IDs (`UZ-UUIDV7-006`)
- 2.1.3 DONE Add integration tests for UUID conversion happy path + invalid legacy conflict path (`UZ-UUIDV7-007`)
- 2.1.4 DONE Ensure no ambiguous parsing in CLI/API output contracts (`UZ-UUIDV7-008`)

### 2.2 Failure contract and operator surfacing

All migration failure points must return stable machine-readable error codes and operator-actionable messages.

**Dimensions:**
- 2.2.1 DONE Migration validation rejects unknown ID shape with stable error code (`UZ-UUIDV7-009`)
- 2.2.2 DONE Migration emits deterministic conflict errors for non-UUIDv7 legacy rows (`UZ-UUIDV7-010`)
- 2.2.3 DONE Rollback guard rails block rollback after UUIDv7 persistence (`UZ-UUIDV7-011`)
- 2.2.4 DONE CLI/API responses include error code and remediation hint (`UZ-UUIDV7-012`)

---

## 3.0 Acceptance Criteria

**Status:** DONE

- [x] 3.1 Migration fails fast with deterministic conflict code when legacy prefixed IDs remain
- [x] 3.2 New rows are issued as UUIDv7 and persisted into UUID-typed columns
- [x] 3.3 Compile/activate/run linkage queries remain deterministic with UUIDv7 IDs
- [x] 3.4 Migration + rollback/test evidence captured in docs/evidence
- [x] 3.5 Migration and API internal failure paths map to stable error codes

---

## 4.0 Verification Commands

**Status:** DONE

```bash
make lint
make test
```

**Dimensions:**
- 4.1 DONE Verify UUIDv7 cutover tests and runtime integration suite pass
- 4.2 DONE Verify rollback guard behavior and deterministic migration conflict failure path
- 4.3 DONE Verify API and CLI emit stable error code fields

---

## 5.0 Error Code Catalog

**Status:** DONE

Use the single base defined at the top of this spec. For each code below, resolve external docs as:

`ERROR_DOCS_BASE + ERROR_CODE`

Examples:
- `UZ-UUIDV7-001` -> `${ERROR_DOCS_BASE}UZ-UUIDV7-001`
- `UZ-UUIDV7-012` -> `${ERROR_DOCS_BASE}UZ-UUIDV7-012`

| Error Code | Spec Contract | Meaning | External Anchor |
|---|---|---|---|
| `UZ-UUIDV7-001` | 1.1 | Migration strategy decision missing or contradictory | `#UZ-UUIDV7-001` |
| `UZ-UUIDV7-002` | 1.2 | Dual-read/dual-write compatibility contract missing | `#UZ-UUIDV7-002` |
| `UZ-UUIDV7-003` | 1.3 | API/DB canonical ID format not enforced | `#UZ-UUIDV7-003` |
| `UZ-UUIDV7-004` | 1.4 | Backfill boundary or ordering rule undefined | `#UZ-UUIDV7-004` |
| `UZ-UUIDV7-005` | 2.1.1 | UUIDv7 schema migration path failed | `#UZ-UUIDV7-005` |
| `UZ-UUIDV7-006` | 2.1.2 | Mixed-generation linkage query failed | `#UZ-UUIDV7-006` |
| `UZ-UUIDV7-007` | 2.1.3 | Interop integration tests failed | `#UZ-UUIDV7-007` |
| `UZ-UUIDV7-008` | 2.1.4 | CLI/API ID parsing ambiguous | `#UZ-UUIDV7-008` |
| `UZ-UUIDV7-009` | 2.2.1 | Unknown ID shape rejected by validation layer | `#UZ-UUIDV7-009` |
| `UZ-UUIDV7-010` | 2.2.2 | Duplicate/conflict during backfill | `#UZ-UUIDV7-010` |
| `UZ-UUIDV7-011` | 2.2.3 | Rollback blocked due to partial transition state | `#UZ-UUIDV7-011` |
| `UZ-UUIDV7-012` | 2.2.4 | Error surfaced without code/remediation contract | `#UZ-UUIDV7-012` |

Implementation note (Mar 12, 2026):
- UUIDv7 default issuance is enforced for new `run_id`, `profile_version_id`, and `compile_job_id`.
- Legacy prefixed runtime generation/validation path has been removed.
- Migration `schema/011_uuidv7_id_migration.sql` converts core ID columns to `UUID`, fails fast on invalid legacy rows (`UZ-UUIDV7-010`), and provides rollback guard (`UZ-UUIDV7-011`).
- API internal error paths are normalized to stable machine-readable error codes (no raw `INTERNAL_ERROR` string responses).

---

## 6.0 Out of Scope

- Broad identifier rewrite outside `run_id`, `profile_version_id`, `compile_job_id`
- Cross-service ID standardization beyond `zombied`
- UI-only identifier presentation changes
