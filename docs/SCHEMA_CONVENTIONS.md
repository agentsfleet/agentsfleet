# Schema Conventions

Canonical reference for agentsfleet database schema patterns. All new tables **must** follow these conventions. Existing tables are brought into compliance when rebuilt.

## Migration Model

**Teardown-rebuild is in force for the M154 rebuild (owner decision, Indy, Jul 31, 2026).** The development database is being dropped and re-created from empty while production is undeployed, so slots `001`–`046` are **retired wholesale** and the schema is re-authored by dependency layer. During this rebuild the frozen-slot rule below does not apply, and `ALTER`/`DROP` statements are forbidden rather than required — a change belongs in the base statement it would have patched. This is the posture RULE SCH already specifies for `VERSION < 2.0.0`; the Jul 22 additive model was the deviation.

**Additive migrations resume once the rebuild lands.** Every subsequent schema change is a **new numbered migration file** — `ALTER TABLE … ADD COLUMN`, new tables, new indexes. **Shipped slot files are then frozen history: never edit an existing `schema/NNN_*.sql`.** Migrations are version-tracked and applied incrementally (expected-vs-applied state is inspectable via `agentsfleetd doctor --schema-gate`). Use `IF NOT EXISTS` guards so a migration is idempotent against both a fresh bootstrap (all slots in order) and an already-provisioned database (new slots only).

Destructive changes (`DROP TABLE`, `DROP COLUMN`, type rewrites) require an explicit owner decision per change — additive is the default an agent may author alone.

> Historical note: slots `001`–`031` predate the additive model (teardown-rebuild with inline DDL edits, enforced by a since-removed `check-schema-gate` lint target). M154 retires them along with everything up to `046`.

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

## Identity Column

**One identity column per table, named `id` (owner decision, Indy, Jul 31, 2026 — M154).** The earlier `uid` rule paired a generated `uid` primary key with a duplicate twin column (`id`, `tenant_id`, `workspace_id`, …) holding the same value under its own `UNIQUE` constraint. That shape is retired.

- **Column:** `id`
- **Type:** Universally Unique Identifier (UUID) `PRIMARY KEY`
- **Generation:** Application-side UUID version 7 (UUIDv7) via `src/agentsfleetd/types/id_format.zig`, never `gen_random_uuid()`.
- **No second unique key on the same value.** This is a correctness rule, not only a storage one: `ON CONFLICT` can arbitrate exactly one constraint, so a table carrying two unique keys over the same value makes two sessions inserting a brand-new row race to a duplicate-key error on the *other* index instead of taking the update arm. `schema/043_runner_lifetime_counters.sql` recorded this before the rule was generalised.
- **Foreign keys reference the primary key**, never a secondary unique constraint.
- **Constraint:** every table carries a UUIDv7 CHECK:
  ```sql
  CONSTRAINT ck_{table}_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7')
  ```
- **Adding a new table:** add a `generate{TableName}Id()` function to `src/agentsfleetd/types/id_format.zig`.
- **API shape:** unchanged — public fields keep exposing `id`, `tenant_id`, `workspace_id` and the other documented names. The column now *is* the public name in most tables, so the aliasing the old rule required mostly disappears; where a public field differs from `id`, alias at the boundary rather than renaming client-facing payloads.
- **Exceptions,** each stated in the slot that creates the table: a curated catalogue keyed by a stable slug (`core.fleet_library`) and a singleton keyed by a pinned integer (`core.model_catalogue_revision`) carry no UUID.

## Timestamps

- **Type:** `BIGINT NOT NULL` — milliseconds since Unix epoch.
- **Generation:** `std.time.milliTimestamp()` in Zig application code.
- **Never** use `TIMESTAMPTZ`, `TIMESTAMP`, or `DEFAULT now()`.

## Standard Columns

Every table must have:

| Column | Type | Required | Notes |
|--------|------|----------|-------|
| `id` | `UUID PRIMARY KEY` | Yes | UUIDv7, app-generated; the table's only identity column |
| `created_at` | `BIGINT NOT NULL` | Yes | Set once at INSERT |
| `updated_at` | `BIGINT NOT NULL` | If mutable | Set at INSERT and every UPDATE |

**Naming is uniform.** `created_at` and `updated_at` are the only names for row lifecycle time — not `recorded_at`, `created_at_ms`, or `updated_at_ms`. A column carrying *domain* time distinct from row birth (for example the originating event's creation instant on a money row) is named for that domain meaning and documented in its slot.

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
