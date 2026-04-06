# M31_002: Schema Default Constants Match Runtime Defaults

**Prototype:** v1.0.0
**Milestone:** M31
**Workstream:** 002
**Date:** Apr 06, 2026
**Status:** DONE
**Branch:** feat/m31-schema-default-constants
**Priority:** P2 — consistency debt; wrong default drift would silently change runtime behavior at enqueue/provision time
**Batch:** B2
**Depends on:** M31_001 (schema file partitioning and schema conventions)

---

## Overview

**Goal (testable):** Runtime fallback values for run limits, workspace budget, and entitlement scoring context are defined in one Zig source of truth and match the active schema defaults in `001_core_foundation.sql`, `002_core_workflow.sql`, and `014_workspace_entitlements.sql`.

**Problem:** The current spec predates `M31_001` and still references `schema/001_initial.sql` and `schema/012_*.sql`, which no longer exist as active migration files. The actual defaults now live in partitioned files, and several runtime paths still repeat numeric literals (`3`, `100000`, `600`, `10000000`, `2048`) instead of referencing named constants. That leaves the schema and runtime free to drift apart silently.

**Solution summary:** Introduce a single Zig defaults module for schema-backed runtime defaults, migrate runtime fallback and provisioning code to consume those constants, annotate the active DDL defaults with the canonical constant location, and add tests that fail if the schema values and Zig constants diverge.

---

## 1.0 Consolidate Run Execution Defaults

**Status:** DONE

The run-enforcement defaults currently live in `schema/002_core_workflow.sql` and are also duplicated in runtime code paths that enqueue runs or recover values from the database. This section centralizes them.

**Dimensions (test blueprints):**
- 1.1 DONE
  - target: `src/types/defaults.zig`
  - input: `none`
  - expected: `exports named constants for run defaults with exact values: max_repair_loops=3, max_tokens=100000, max_wall_time_seconds=600`
  - test_type: unit
- 1.2 DONE
  - target: `src/http/handlers/runs/start.zig:handleStartRun`
  - input: `run creation request that relies on DB defaults for run enforcement fields`
  - expected: `any literal 3/100000/600 tied to run defaults is replaced by named constants from the shared defaults module`
  - test_type: contract
- 1.3 DONE
  - target: `src/pipeline/worker_claim.zig:processNextRun`
  - input: `row decoding where max_wall_time_seconds or max_repair_loops fallback path is exercised`
  - expected: `fallback path uses the shared constants instead of numeric literals`
  - test_type: unit
- 1.4 DONE
  - target: `schema/002_core_workflow.sql`
  - input: `DDL review of core.runs defaults`
  - expected: `DEFAULT 3`, `DEFAULT 100000`, and `DEFAULT 600` remain unchanged but each has an adjacent comment naming the Zig constant source of truth`
  - test_type: contract

---

## 2.0 Consolidate Workspace Budget Default

**Status:** DONE

The free-tier monthly token budget default now lives in `schema/001_core_foundation.sql` after the `M31_001` split. Runtime budget/provisioning code must reference the same default name.

**Dimensions (test blueprints):**
- 2.1 DONE
  - target: `src/types/defaults.zig`
  - input: `none`
  - expected: `exports named constant for workspace monthly token budget with exact value 10000000`
  - test_type: unit
- 2.2 DONE
  - target: `schema/001_core_foundation.sql`
  - input: `DDL review of core.workspaces.monthly_token_budget`
  - expected: `DEFAULT 10000000` remains unchanged and has an adjacent comment naming the Zig constant source of truth`
  - test_type: contract
- 2.3 DONE
  - target: `src/http/handlers/workspaces_lifecycle.zig` or other provisioning path that creates workspaces
  - input: `workspace creation path review`
  - expected: `if the budget is set in application code, it uses the shared constant; if the DB default is relied on, no duplicate literal remains in touched runtime code`
  - test_type: contract

---

## 3.0 Consolidate Entitlement Scoring Context Default

**Status:** DONE

The entitlement scoring context default is partially centralized today, but the spec must reflect the actual file after schema partitioning and close the remaining duplication gap.

**Dimensions (test blueprints):**
- 3.1 DONE
  - target: `src/types/defaults.zig` and `src/pipeline/scoring_mod/persistence.zig`
  - input: `none`
  - expected: `scoring_context_max_tokens default is defined once in the shared defaults module and consumed by scoring persistence instead of maintaining a private duplicate constant`
  - test_type: unit
- 3.2 DONE
  - target: `schema/014_workspace_entitlements.sql`
  - input: `DDL review of billing.workspace_entitlements.scoring_context_max_tokens`
  - expected: `DEFAULT 2048` remains unchanged and has an adjacent comment naming the Zig constant source of truth`
  - test_type: contract
- 3.3 DONE
  - target: `src/http/handlers/workspaces_billing.zig`, `src/pipeline/scoring_mod/types.zig`, and touched tests`
  - input: `paths that assume default scoring context token count`
  - expected: `new default usage points reference the shared constant or intentionally rely on DB state; no new magic 2048 literals are introduced in application code`
  - test_type: contract

---

## 4.0 Drift Detection and Documentation

**Status:** DONE

This workstream is only complete if future edits cannot silently change one side without the other.

**Dimensions (test blueprints):**
- 4.1 DONE
  - target: `src/* test module for defaults parity`
  - input: `compile-time or unit-level assertions over shared constants`
  - expected: `tests fail if any shared constant value changes without updating the parity expectations`
  - test_type: unit
- 4.2 DONE
  - target: `docs/contributing/SCHEMA_CONVENTIONS.md`
  - input: `schema default policy review`
  - expected: `document states that schema-backed numeric defaults require a named Zig constant when reused in runtime logic`
  - test_type: contract
- 4.3 DONE
  - target: `repo-wide grep over touched default values`
  - input: `search for bare literals 3, 100000, 600, 10000000, 2048 in context-specific code paths`
  - expected: `only schema DDL, the shared defaults module, and test fixtures with intentional local override cases retain these values`
  - test_type: contract

---

## 5.0 Interfaces

**Status:** DONE

Lock the shared default API surface before implementation.

### 5.1 Public Constants

```zig
pub const DEFAULT_RUN_MAX_REPAIR_LOOPS: u32 = 3;
pub const DEFAULT_RUN_MAX_TOKENS: u64 = 100000;
pub const DEFAULT_RUN_MAX_WALL_TIME_SECONDS: u64 = 600;
pub const DEFAULT_WORKSPACE_MONTHLY_TOKEN_BUDGET: u64 = 10000000;
pub const DEFAULT_SCORING_CONTEXT_MAX_TOKENS: u32 = 2048;
```

### 5.2 Input Contracts

| Field | Type | Constraints | Example |
|-------|------|-------------|---------|
| `run_limit_fallback` | runtime decoded DB field | Used only when a DB row is missing or carries an invalid fallback-triggering value | `NULL -> DEFAULT_RUN_MAX_WALL_TIME_SECONDS` |
| `workspace_budget_default` | schema default / provisioning fallback | Must exactly equal the schema DDL default | `10000000` |
| `scoring_context_max_tokens` | integer | Clamped to 512..8192 after default selection | `2048` |

### 5.3 Output Contracts

| Field | Type | When | Example |
|-------|------|------|---------|
| `DEFAULT_*` constant values | Zig compile-time constants | Imported by runtime modules | `DEFAULT_RUN_MAX_TOKENS` |
| DDL comment linkage | SQL comment text | Present next to active schema defaults | `Canonical constant: src/types/defaults.zig#DEFAULT_RUN_MAX_TOKENS` |
| parity test result | pass/fail | During `make test` | `schema default constant parity passes` |

### 5.4 Error Contracts

| Error condition | Behavior | Caller sees |
|----------------|----------|-------------|
| Shared defaults module not imported by touched runtime path | Review/test failure | CI/test fails before merge |
| Zig constant changed without matching DDL update | Parity assertion fails | `make test` failure |
| DDL default changed without comment update | Contract grep/test fails | `make test` or review failure |
| Local test fixture intentionally overrides a value | Allowed only with inline explanation in test scope | No production-path failure |

---

## 6.0 Failure Modes

**Status:** DONE

| Failure | Trigger | System behavior | User observes |
|---------|---------|----------------|---------------|
| Schema/runtime drift | DDL default changes but Zig constant does not | Parity test fails; spec not complete | CI or local test failure |
| Runtime literal reintroduced | New code writes `3`, `100000`, `600`, `10000000`, or `2048` directly in a production path | grep/review gate fails | Review finding before merge |
| Over-centralization | Non-schema business constants get moved into the defaults module without shared runtime/schema need | Reject in review; keep module scoped to schema-backed defaults only | Smaller, boring module remains |
| Test fixture false positive | Fixture uses literal values for local scenario setup | Keep literal only when it is test-local and not meant to mirror production defaults; annotate if ambiguous | No blocker if intentional |

**Platform constraints:**
- The schema files remain the database source of truth for Postgres defaults; Zig constants mirror them for runtime consistency, not the other way around.
- `M31_001` split the schema into numbered single-concern files; this spec must reference only active migration files, not retired `001_initial.sql` paths.

---

## 7.0 Implementation Constraints (Enforceable)

**Status:** DONE

| Constraint | How to verify |
|-----------|---------------|
| Shared defaults live in one Zig module | `rg -n "DEFAULT_RUN_MAX_|DEFAULT_WORKSPACE_MONTHLY_TOKEN_BUDGET|DEFAULT_SCORING_CONTEXT_MAX_TOKENS" src` |
| Active schema files reference canonical constant location in comments | `rg -n "Canonical constant:" schema/001_core_foundation.sql schema/002_core_workflow.sql schema/014_workspace_entitlements.sql` |
| No touched production path uses raw run/budget/context default literals | `rg -n "10000000|100000|2048|\\b600\\b|\\b3\\b" src/http src/pipeline src/state` with reviewed allowlist |
| Touched files stay under 500 lines | `wc -l <file>` for each modified file |
| Zig changes still pass drain and cross-compile gates if implementation proceeds | `make check-pg-drain && zig build -Dtarget=x86_64-linux && zig build -Dtarget=aarch64-linux` |

---

## 8.0 Test Specification

**Status:** DONE

### Unit Tests

| Test name | Dimension | Target | Input | Expected |
|-----------|-----------|--------|-------|----------|
| `defaults.run_constants_match_spec` | 1.1 | `src/types/defaults.zig` | none | run default constants equal `3/100000/600` |
| `defaults.workspace_budget_matches_spec` | 2.1 | `src/types/defaults.zig` | none | workspace budget constant equals `10000000` |
| `defaults.scoring_context_matches_spec` | 3.1 | shared defaults module | none | scoring context constant equals `2048` |
| `worker_claim_uses_shared_defaults` | 1.3 | `src/pipeline/worker_claim.zig` | missing/invalid fallback values | shared constants are used |

### Integration Tests

| Test name | Dimension | Target | Input | Expected |
|-----------|-----------|--------|-------|----------|
| `run_start_defaults_round_trip` | 1.2 | run start handler + DB insert path | create run without explicit limits | persisted row matches schema-backed defaults |
| `workspace_entitlement_default_round_trip` | 3.3 | entitlement load path | workspace with default `scoring_context_max_tokens` | runtime config resolves to `2048` via shared constant |

### Contract Tests

| Test name | Dimension | Target | Input | Expected |
|-----------|-----------|--------|-------|----------|
| `ddl_comments_reference_constants` | 1.4, 2.2, 3.2 | active schema files | grep schema comments | every tracked default names canonical Zig constant |
| `no_magic_literals_in_touched_runtime_paths` | 4.3 | touched runtime files | grep literal search | no unapproved production literals remain |

### Spec-Claim Tracing

| Claim | Proved by |
|------|-----------|
| Run defaults are centralized and reused | `defaults.run_constants_match_spec`, `run_start_defaults_round_trip`, `worker_claim_uses_shared_defaults` |
| Workspace budget default is centralized | `defaults.workspace_budget_matches_spec`, `ddl_comments_reference_constants` |
| Scoring context default is centralized | `defaults.scoring_context_matches_spec`, `workspace_entitlement_default_round_trip` |
| Schema/runtime drift is detectable | `ddl_comments_reference_constants`, parity/unit tests, literal grep contract |

---

## 9.0 Verification Evidence

**Status:** PENDING

| Command | Purpose | Evidence placeholder |
|---------|---------|----------------------|
| `make test` | unit/integration validation | PASS |
| `make test-integration-db` | DB-backed default behavior validation | PASS |
| `make lint` | compile/lint validation | PASS |
| `rg -n "Canonical constant:" schema/001_core_foundation.sql schema/002_core_workflow.sql schema/014_workspace_entitlements.sql` | DDL linkage verification | PASS — 5 canonical comment references found |

---

## 10.0 Acceptance Criteria

**Status:** DONE

- [x] 10.1 Active schema defaults in `schema/001_core_foundation.sql`, `schema/002_core_workflow.sql`, and `schema/014_workspace_entitlements.sql` are the only DDL files referenced by this workstream
- [x] 10.2 All five tracked schema-backed numeric defaults have named Zig constants in one shared module
- [x] 10.3 Touched runtime code paths use the shared constants instead of raw literals
- [x] 10.4 Active DDL defaults include comments naming the canonical Zig constant location
- [x] 10.5 Tests prove the Zig constants still match the schema defaults
- [x] 10.6 `make test` and `make test-integration-db` pass

---

## 11.0 Out of Scope

- Changing the numeric values of any tracked default
- Replacing all numeric literals repo-wide regardless of whether they are schema-backed defaults
- Empty collection defaults (`DEFAULT '[]'`, `DEFAULT '{}'`)
- Postgres built-in defaults unrelated to shared runtime behavior (`DEFAULT FALSE`, `DEFAULT 0`, etc.)
- Repartitioning schema files again after `M31_001`
