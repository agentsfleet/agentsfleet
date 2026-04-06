# M31_001: Schema Standardization

**Prototype:** v1.0.0
**Milestone:** M31
**Workstream:** 001
**Date:** Apr 05, 2026
**Status:** DONE
**Branch:** feat/m31-schema-standardization
**Priority:** P2 — consistency debt, no production breakage
**Batch:** B2
**Depends on:** None

---

## Overview

Schema audit (Apr 05, 2026) revealed inconsistencies across 34 tables in 4 schemas. Since tables are rebuilt from scratch (no ALTER migrations), these can be fixed in a single pass.

---

## 1.0 Standardize `platform_llm_keys` Table

**Status:** DONE

`core.platform_llm_keys` uses `gen_random_uuid()` (UUIDv4) and `TIMESTAMPTZ DEFAULT now()` instead of the project standard: UUIDv7 + BIGINT milliseconds.

**Dimensions:**
- 1.1 DONE `id` is now application-generated UUIDv7 (`UUID PRIMARY KEY`, no DB default)
- 1.2 DONE `created_at` is now `BIGINT NOT NULL`
- 1.3 DONE `updated_at` is now `BIGINT NOT NULL`
- 1.4 DONE `generatePlatformLlmKeyId()` added to `src/types/id_format.zig`
- 1.5 DONE INSERT/UPDATE paths now write `std.time.milliTimestamp()`

---

## 2.0 Add Missing `updated_at` Columns

**Status:** DONE

13 tables lack `updated_at`. Append-only/event tables (where UPDATE is blocked by trigger) are exempt. Mutable tables should have `updated_at`.

**Tables needing `updated_at` (mutable, not append-only):**
- 2.1 DONE `core.tenants` now includes `updated_at BIGINT NOT NULL`
- 2.2 DONE `core.gate_results` made append-only with UPDATE/DELETE trigger (exempt from `updated_at`)
- 2.3 DONE `core.workspace_memories` now includes `updated_at BIGINT NOT NULL`

**Tables exempt (append-only or event-sourced):**
- `core.policy_events` — uses `ts`, append-only by design
- `billing.usage_ledger` — append-only ledger
- `billing.entitlement_policy_audit_snapshots` — audit log
- `billing.workspace_billing_audit` — audit log
- `billing.workspace_credit_audit` — audit log
- `agent.config_linkage_audit_artifacts` — has append-only trigger
- `agent.workspace_latency_baseline` — uses `computed_at`, replace-on-compute pattern
- `agent.agent_run_scores` — uses `scored_at`, write-once per run
- `agent.agent_run_analysis` — uses `analyzed_at`, write-once per run
- `audit.ops_ro_access_events` — uses `accessed_at`, audit log

---

## 3.0 Audit Trail: `updated_by` Column

**Status:** DONE

No table has `updated_by`. Changes are tracked via `actor` in separate event/transition tables. Evaluate whether direct-mutation tables should have `updated_by`.

**Dimensions:**
- 3.1 DONE Evaluated: no inline `updated_by` for `core.workspaces`, `core.specs`, `core.runs`
- 3.2 DONE Not applied by design (decision is "no")
- 3.3 DONE Decision documented in `docs/contributing/SCHEMA_CONVENTIONS.md`

---

## 4.0 Document Schema Conventions

**Status:** DONE

Create `docs/contributing/SCHEMA_CONVENTIONS.md` as the canonical reference for:

- 4.1 DONE ID format documented: UUIDv7 via `src/types/id_format.zig`
- 4.2 DONE Timestamp policy documented: BIGINT milliseconds
- 4.3 DONE Standard column policy documented
- 4.4 DONE Audit pattern documented (`actor` + append-only triggers where needed)
- 4.5 DONE RNG policy documented (`std.crypto.random` via UUIDv7 helpers)

---

## 5.0 Acceptance Criteria

**Status:** DONE

- [x] 5.1 `platform_llm_keys` uses UUIDv7 + BIGINT timestamps
- [x] 5.2 `tenants`, `gate_results`, `workspace_memories` standardized (`updated_at` on mutable tables; append-only trigger on `gate_results`)
- [x] 5.3 `docs/contributing/SCHEMA_CONVENTIONS.md` exists and is complete
- [x] 5.4 `make test` and `make test-integration-db` pass
- [x] 5.5 No `gen_random_uuid()` or `TIMESTAMPTZ DEFAULT now()` in active migration files

### Verification Evidence (Apr 05, 2026)

- `make lint` ✅
- `make test` ✅
- `make build` ✅
- `make check-pg-drain` ✅
- `make test-integration-db` ✅

---

## 6.0 Out of Scope

- Adding `updated_by` to existing rows retroactively
- Changing append-only/event tables to add `updated_at`
- Migrating existing data (tables rebuilt from scratch)
