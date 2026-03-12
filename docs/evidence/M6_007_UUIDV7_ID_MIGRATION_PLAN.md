# M6_007 UUIDv7 ID Migration Evidence

**Workstream:** M6_007  
**Date:** Mar 12, 2026  
**Status:** DONE

## 1.0 Scope Closure Evidence

- UUID issuance and validation are UUIDv7-only in [`src/types/id_format.zig`](/Users/kishore/Projects/usezombie/src/types/id_format.zig).
- Core migration cutover is implemented in [`schema/011_uuidv7_id_migration.sql`](/Users/kishore/Projects/usezombie/schema/011_uuidv7_id_migration.sql):
  - validates legacy text rows are canonical UUIDv7 before conversion
  - converts core ID columns from `TEXT` to `UUID`
  - rebinds FK constraints
  - adds deterministic rollback guard (`assert_uuidv7_rollback_allowed`)
- Canonical migration ordering now includes trace context and UUID cutover:
  - [`schema/embed.zig`](/Users/kishore/Projects/usezombie/schema/embed.zig)
  - [`src/cmd/common.zig`](/Users/kishore/Projects/usezombie/src/cmd/common.zig)

## 2.0 Error Code Normalization Evidence

- Internal catch/error responses no longer emit raw `"INTERNAL_ERROR"` in HTTP handlers.
- Stable global codes are defined in [`src/errors/codes.zig`](/Users/kishore/Projects/usezombie/src/errors/codes.zig), including:
  - internal failures (`UZ-INTERNAL-00x`)
  - auth/session failures (`UZ-AUTH-00x`)
  - request/workspace/run/profile errors
  - UUID migration errors (`UZ-UUIDV7-00x`)
- Session completion error mapping now uses stable codes in [`src/http/handler.zig`](/Users/kishore/Projects/usezombie/src/http/handler.zig).

## 3.0 Verification Commands

Executed:

```bash
make lint
make test
```

Observed result:

- `make lint` passed (`zombied` + website lint/typecheck).
- `make test` passed full gate:
  - Zig tests: `140 passed`, `2 skipped`
  - zombiectl tests: `15 pass`
  - website tests: `22 files / 179 tests passed`
  - app tests: `2 files / 4 tests passed`
  - backend/API e2e lane passed

## 4.0 Migration Test Note

- UUID migration integration tests are present in [`src/db/pool.zig`](/Users/kishore/Projects/usezombie/src/db/pool.zig) and gated behind `UUID_MIGRATION_TESTS=1` to avoid hard-failing default CI/local runs when no Postgres test endpoint is configured.
