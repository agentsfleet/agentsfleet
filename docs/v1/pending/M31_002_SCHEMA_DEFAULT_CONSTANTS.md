# M31_002: Schema Default Constants

**Prototype:** v1.0.0
**Milestone:** M31
**Workstream:** 002
**Date:** Apr 05, 2026
**Status:** PENDING
**Priority:** P2 — consistency debt, no production breakage
**Batch:** B2
**Depends on:** None

---

## Overview

Schema audit (Apr 05, 2026) found 5 magic numeric DEFAULT values scattered across DDL files. Each should have a corresponding named constant in Zig that serves as the single source of truth. The DDL defaults must match the Zig constants so behavior is obvious and changes happen in one place.

Empty collection defaults (`DEFAULT '[]'`, `DEFAULT '{}'`) are standard patterns and are not magic numbers — they are excluded from this workstream.

---

## 1.0 Extract Run Enforcement Defaults

**Status:** PENDING

Three magic numbers in `schema/001_initial.sql` govern run enforcement limits. Extract each to a named constant in the appropriate Zig module.

| DDL column | Current DDL default | Proposed constant |
|---|---|---|
| `max_repair_loops` | `DEFAULT 3` | `default_max_repair_loops` |
| `max_tokens` | `DEFAULT 100000` | `default_max_tokens` |
| `max_wall_time_seconds` | `DEFAULT 600` | `default_max_wall_time_seconds` |

**Dimensions:**
- 1.1 PENDING Create named constants in the Zig source (e.g. `src/types/defaults.zig` or colocated with run logic)
- 1.2 PENDING Verify every INSERT / fallback path that references these values uses the constant, not a literal
- 1.3 PENDING Add a comment in `schema/001_initial.sql` next to each DEFAULT noting the canonical constant location
- 1.4 PENDING Unit test: each constant matches the value used in the DDL

---

## 2.0 Extract Workspace Budget Default

**Status:** PENDING

`monthly_token_budget` in `schema/001_initial.sql` defaults to `10000000`. This controls the free-tier token budget and must be a named constant.

**Dimensions:**
- 2.1 PENDING Create `default_monthly_token_budget` constant in Zig
- 2.2 PENDING Update all INSERT / provisioning paths to use the constant instead of a literal
- 2.3 PENDING Add DDL comment referencing the canonical constant

---

## 3.0 Extract Entitlement Defaults

**Status:** PENDING

`scoring_context_max_tokens` in `schema/012_*.sql` defaults to `DEFAULT 2048`. This belongs with the entitlement config constants.

**Dimensions:**
- 3.1 PENDING Create `default_scoring_context_max_tokens` constant in Zig
- 3.2 PENDING Update all INSERT / entitlement provisioning paths to use the constant
- 3.3 PENDING Add DDL comment referencing the canonical constant

---

## 4.0 Acceptance Criteria

**Status:** PENDING

- [ ] 4.1 All 5 magic DEFAULT values have a corresponding named constant in Zig
- [ ] 4.2 No bare numeric literal for these values remains in application code (`grep` confirms zero hits outside `schema/` and the constants file)
- [ ] 4.3 DDL DEFAULT values match their Zig constants (verified by test or static assertion)
- [ ] 4.4 `make test` and `make test-integration-db` pass

---

## 5.0 Out of Scope

- Changing the actual numeric values of any default
- Empty collection defaults (`DEFAULT '[]'`, `DEFAULT '{}'`)
- Defaults managed by Postgres itself (e.g. `DEFAULT TRUE`, `DEFAULT 0` for counters)
- Refactoring DDL files into a different layout or splitting migrations
