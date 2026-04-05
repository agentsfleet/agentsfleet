# Schema Conventions

Canonical reference for UseZombie database schema patterns. All new tables **must** follow these conventions. Existing tables are brought into compliance when rebuilt.

## Migration Model

**Until v0.5.0:** Full teardown-rebuild. Tables are dropped and recreated from scratch on every deploy. No `ALTER TABLE` migrations. Schema changes are made inline in the DDL files.

**After v0.5.0:** ALTER migrations required. Schema changes must be backward-compatible and additive. This document will be updated with migration rules when that transition happens.

## Schema File Organization

- Each SQL file must be **≤100 lines** and **single-concern** (one table or one logical group).
- Files are numbered sequentially: `001_core_foundation.sql`, `002_core_workflow.sql`, etc.
- When splitting a file, slide subsequent file numbers to maintain order.
- Every SQL file must be registered in `schema/embed.zig` (compile-time embed) and `src/cmd/common.zig` (migration version array).
- No-op stub files (e.g., columns folded into earlier files) are kept for version history but excluded from the migrations array.

## ID Format

- **Type:** `UUID PRIMARY KEY`
- **Generation:** Application-side UUIDv7 via `src/types/id_format.zig`, never `gen_random_uuid()`.
- **Constraint:** Every table must have a UUIDv7 CHECK constraint:
  ```sql
  CONSTRAINT ck_{table}_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7')
  ```
- **Adding a new table:** Add a `generate{TableName}Id()` function to `src/types/id_format.zig`.

## Timestamps

- **Type:** `BIGINT NOT NULL` — milliseconds since Unix epoch.
- **Generation:** `std.time.milliTimestamp()` in Zig application code.
- **Never** use `TIMESTAMPTZ`, `TIMESTAMP`, or `DEFAULT now()`.

## Standard Columns

Every table must have:

| Column | Type | Required | Notes |
|--------|------|----------|-------|
| `id` | `UUID PRIMARY KEY` | Yes | UUIDv7, app-generated |
| `created_at` | `BIGINT NOT NULL` | Yes | Set once at INSERT |
| `updated_at` | `BIGINT NOT NULL` | If mutable | Set at INSERT and every UPDATE |

**Mutable tables** (any table where UPDATE is a valid operation) must have `updated_at`.

**Append-only/event tables** (where UPDATE is blocked by trigger or by design) are exempt from `updated_at`.

## Audit Pattern

- **Actor tracking:** Use `actor TEXT` in event/transition tables (e.g., `policy_events`, `usage_ledger`).
- **No `updated_by` column.** Changes to mutable tables are tracked via separate event-sourced audit tables (e.g., `workspace_billing_audit`, `harness_change_log`), not inline `updated_by`.
- **Append-only enforcement:** Tables that must never be updated should have a trigger:
  ```sql
  CREATE OR REPLACE FUNCTION core.{table}_append_only() RETURNS trigger AS $$
  BEGIN
      RAISE EXCEPTION '{table} is append-only — UPDATE and DELETE are not permitted';
  END;
  $$ LANGUAGE plpgsql;

  CREATE TRIGGER trg_{table}_append_only
      BEFORE UPDATE OR DELETE ON core.{table}
      FOR EACH ROW EXECUTE FUNCTION core.{table}_append_only();
  ```

## RNG

- **Always** use `std.crypto.random` (via `allocUuidV7` in `id_format.zig`).
- **Never** use custom RNG implementations or `std.rand`.
