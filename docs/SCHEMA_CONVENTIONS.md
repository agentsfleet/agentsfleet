# Schema Conventions

Canonical reference for agentsfleet database schema patterns. All new tables **must** follow these conventions. Existing tables are brought into compliance when rebuilt.

## Migration Model

**Additive migrations (current model, owner decision Jul 22, 2026).** Every schema change lands as a **new numbered migration file** — `ALTER TABLE … ADD COLUMN`, new tables, new indexes. **Shipped slot files are frozen history: never edit an existing `schema/NNN_*.sql`.** Migrations are version-tracked and applied incrementally (expected-vs-applied state is inspectable via `agentsfleetd doctor --schema-gate`). Use `IF NOT EXISTS` guards so a migration is idempotent against both a fresh bootstrap (all slots in order) and an already-provisioned database (new slots only).

Destructive changes (`DROP TABLE`, `DROP COLUMN`, type rewrites) still require an explicit owner decision per change — additive is the default an agent may author alone.

> Historical note: slots `001`–`031` predate this model (teardown-rebuild with inline DDL edits, enforced by a since-removed `check-schema-gate` lint target). They remain valid bootstrap history and are equally frozen.

## Schema File Organization

- Each SQL file must be **≤100 lines** and **single-concern** (one table, one logical group, or one additive change).
- Files are numbered sequentially: `001_core_foundation.sql`, `002_core_workflow.sql`, etc. New migrations append the next number; shipped numbers are never reused or slid.
- Every SQL file must be registered in `schema/embed.zig` (compile-time embed) and `src/agentsfleetd/cmd/common.zig` (migration version array).
- No-op stub files (e.g., columns folded into earlier files) are kept for version history but excluded from the migrations array.

## SQL Qualification

- Use schema-qualified table names in SQL (`core.platform_llm_keys`, `core.workspaces`, etc.) for new queries and handlers.
- Do not rely on session `search_path` defaults for correctness.
- Legacy unqualified queries may remain temporarily, but touched paths should be migrated to schema-qualified names.

## Schema-Backed Runtime Defaults

- If a numeric schema default is also used in runtime fallback or provisioning logic, define it once in Zig as a named constant and import that constant in runtime code.
- Keep the database DDL value unchanged unless the product default is intentionally changing; the Zig constant mirrors the schema default for drift detection.
- Add an adjacent `Canonical constant:` SQL comment next to each shared numeric default so reviewers can verify the linkage quickly.

## Unique Identifier (UID) Format

- **Column:** `uid`
- **Type:** Universally Unique Identifier (UUID) `PRIMARY KEY`
- **Generation:** Application-side UUID version 7 (UUIDv7) via `src/agentsfleetd/types/id_format.zig`, never `gen_random_uuid()`.
- **Constraint:** Every table must have a UUIDv7 CHECK constraint:
  ```sql
  CONSTRAINT ck_{table}_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7')
  ```
- **Adding a new table:** Add a `generate{TableName}Id()` function to `src/agentsfleetd/types/id_format.zig`.
- **API shape:** public API fields may continue to expose `id`, `tenant_id`, `workspace_id`, or other documented names. SQL should alias `uid` back to the public field name at the boundary instead of casually renaming client-facing payloads.

## Timestamps

- **Type:** `BIGINT NOT NULL` — milliseconds since Unix epoch.
- **Generation:** `std.time.milliTimestamp()` in Zig application code.
- **Never** use `TIMESTAMPTZ`, `TIMESTAMP`, or `DEFAULT now()`.

## Standard Columns

Every table must have:

| Column | Type | Required | Notes |
|--------|------|----------|-------|
| `uid` | `UUID PRIMARY KEY` | Yes | UUIDv7, app-generated |
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
