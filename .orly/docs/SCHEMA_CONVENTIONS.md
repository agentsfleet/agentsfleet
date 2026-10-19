# Schema Conventions

## Repository policy

Read the repository's schema sources, migration registration and deployment
policy before changing its database. Preserve shipped migrations and live data;
destructive changes need the owner's approval for the specific change.
Never infer a disposable database from a version threshold in another product.

Use the repository's naming, identity and timestamp conventions consistently.
Qualify table names, grant only the operations each runtime role needs, and test
with the real schema through shared fixture helpers.
Keep migration registration and position assertions synchronized with each edit.

Canonical reference for `agentsfleet` database schema patterns. All new tables **must** follow these conventions. The table-by-table layout lives in the product repository's `docs/architecture/postgres_schemas.md`; this section carries the rules that layout follows.

## Migration Model

**Additive at `VERSION >= 0.30.0`.** Every schema change is a new numbered slot; shipped slots are frozen history. Destructive changes (`DROP TABLE`, `DROP COLUMN`, type rewrites) need an explicit owner decision per change. A from-empty rebuild past `0.30.0` happens only under an owner-recorded `SCHEMA GUARD: SKIPPED per user override (reason: …)` with the database dropped first — the M229 rebuild (Oct 17, 2026) is the precedent.

## Migrator

SQLx's `Migrator` applies the slots from the explicit list in `rustd/crates/afd_db/src/migration.rs` (`Migrator::with_migrations`; the `migrate` feature only, `macros` stays off). It keeps checksums, so an edited applied slot is refused, and records the ledger in the `audit` schema. Locking is ours: SQLx's lock is off (`set_locking(false)`) and a bounded `pg_try_advisory_lock` poll wraps the run, so a held lock fails loudly instead of hanging a deploy.

## Schema File Organization

- One slot per table: the table, its local constraints, indexes and grants together; each file **≤100 lines**.
- Slots are ordered by foreign key — a table follows every table it references, across schemas. A foreign key that closes a cycle (a head pointer, for example) gets its own registered slot after both tables exist, `DEFERRABLE INITIALLY DEFERRED`.
- Every slot is registered in `rustd/crates/afd_db/src/migration.rs`; the `rust-migration-list` lint compares the list to the tracked files.
- Shipped numbers are never reused or slid.

## Placement and naming

1. **Placement.** A table lives in the schema of the product area that owns it (`identity`, `authoring`, `fleet`, `runner`, `connector`, `model`, `billing`, `vault`, `memory`). Its name is a plural noun that never repeats the schema name: `authoring.drafts`, not `authoring.authoring_drafts`. A singleton is named for its one row and says so in its slot.
2. **Foreign keys** end `_id` and name the table they point at (`user_id`, `draft_id`). A prefix names the role when the column alone would not say it (`invited_by_user_id`, `active_grant_id`, `last_runner_id`). A deliberately missing foreign key is explained in the slot comment.
3. **Status** columns are named `status`; categories are `<thing>_type`. RULE STS applies in full and without override: no literal that names a value or identifier the application also names — a `DEFAULT`, a `CHECK (col IN (…))` list, a trigger comparison against a column, a `current_setting()` parameter name, or `RAISE EXCEPTION` text a caller matches on. A predicate that needs a state reads a NULL marker column; a privileged path (a purge, for example) is a grant, not a session setting. The only literals left are structural ones nothing in the application names: the UUID version nibble `'7'`, `TG_OP` values, and exception text no caller matches on.
4. **Timestamps.** Every table has `created_at`; every table whose rows change has `updated_at`. Any other time column is one of: (a) a deadline named `*expires_at`; (b) a NULL marker an index or trigger reads; (c) a domain instant the API returns that `updated_at` cannot carry; (d) an accounting cursor billing reads.
5. **One fact, one home.** No JSON blob copies a column; no counter restates another table's number. Named exceptions, each with its consistency rule in the slot: an aggregate maintained under the same lock or trigger as its source; a delivery snapshot that must outlive its source; a non-secret projection of encrypted content; a scope copy a restricted role needs to filter alone.
6. **Retry keys** (`operation_id` plus a request digest) live only in receipt tables.
7. **Every column names its reader** before it is created. Accounting and audit records are exempt when the slot names the export or verification that reads them.
8. **A new table** carries a written answer to "why not a column, or an existing table".
9. **Immutable tables** carry no `updated_at`, no UPDATE grant, and a trigger that refuses UPDATE. DELETE stays governed by grants and the deletion matrix, so cascades keep working.

### Counter and pointer names

| Kind | Name |
|---|---|
| Position within a parent (1, 2, 3…) | `sequence` |
| Pointer to another table's position | `<thing>_sequence` |
| Highest position so far | `last_<thing>_sequence` |
| "Everything up to N" watermark | `<verb>_through_sequence` |
| Global insertion order | `sequence` (identity column) |
| Frozen content version | `revision`; pointer `<x>_revision_id`; the live one `head_revision_id` |
| Compare-and-swap edit counter | `resource_version`, or `<scope>_version` when it covers one action |
| Lease epoch | `fencing_token` |
| Tally | `<thing>_count` |
| Deadline | `*expires_at` |

Wire field names (API and runner protocol) are not renamed by these rules.

## Deletion matrix

Every root a user or operator can delete — draft, user, Library card, fleet, workspace, tenant — has a stated row in the product's deletion matrix: what goes with it, what stays, and what is scrubbed at once. Money records (charges, settlements, the attempts they hang off) never cascade from an erasable owner; they keep opaque identifiers. Private text is scrubbed on every erasure path, not only the one that purges drafts.

## SQL Qualification

- Use schema-qualified table names in SQL (`identity.workspaces`, `fleet.fleets`).
- Do not rely on session `search_path` defaults for correctness.

## Runtime SQL

- Every static statement lives in a `.sql` file beside the use case that runs it (for example `library/sql/list.sql`), loaded with `include_str!` and bound by a typed Rust function that states its parameters, authorization and locks.
- Rows decode into named `FromRow` types selected by explicit column lists; `Json<T>` is for real JSON columns only; storage rows stay separate from wire types. No `to_jsonb(<row>)` whole-row reads.
- One integration test `PREPARE`s every `.sql` file against the rebuilt schema, so a stale table or column fails before merge. A statement that must be assembled at run time is named in its module and covered by its own test.
- Domain stores take the caller's connection or transaction; there is no generic repository trait.
- Every pooled runtime connection carries `lock_timeout` and `statement_timeout` named constants; the migrate connection carries neither.

## Schema-Backed Runtime Defaults

- A numeric schema default also used in runtime fallback or provisioning logic is defined once as a Rust named constant in the owning crate and referenced by the slot comment (`Canonical constant: crate::path::NAME`).
- Keep the DDL value unchanged unless the product default is intentionally changing.

## Identity Column

**One identity column per table, named `id` (owner decision, Indy, Jul 31, 2026 — M154).**

- **Type:** Universally Unique Identifier (UUID) `PRIMARY KEY`, application-minted UUID version 7 (UUIDv7) through `afd_core::id`, never `gen_random_uuid()`.
- **No second unique key on the same value.** `ON CONFLICT` arbitrates exactly one constraint, so two unique keys over one value make concurrent inserts race to a duplicate-key error on the other index.
- **Foreign keys reference the primary key**, never a secondary unique constraint.
- **Constraint:** every UUID-keyed table carries `CONSTRAINT ck_{table}_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7')`.
- **Exceptions,** each stated in its slot: a row that exists at most once per parent is keyed by the parent (`fleet.sessions`, `fleet.activity_counters`, `runner.fleet_slots`, `runner.lifetime_counters`, `billing.wallets`, `model.tenant_selections`); a singleton keyed by a pinned integer (`model.catalogue_version`); a natural key addressed by every statement (`model.platform_defaults` by `provider`).

## Timestamps

- **Type:** `TIMESTAMPTZ(3) NOT NULL` (nullable for a NULL marker), so the database, the Rust `jiff::Timestamp` and the API agree on millisecond precision. Never `BIGINT` epoch integers or `TIMESTAMP` without a time zone.
- **Server lifecycle stamps** (`created_at`, `updated_at`) are written by the statement as `now()`: one database clock, one instant per transaction. No column `DEFAULT`; every write names its stamp in its `.sql` file.
- **Deadlines** are computed in SQL as `clock_timestamp() + $ttl::interval`, with the TTL bound as an exact interval.
- **A locking validity check** ("is this lease still live?") locks the target rows first, then reads `clock_timestamp()` once in a `MATERIALIZED` step that depends on the locked rows. A clock read in the `WHERE` or select list of a locking statement can run before the lock wait and accept an expired row.
- **A non-locking read or `SKIP LOCKED` sweep** compares against `statement_timestamp()`, which an index can use; `clock_timestamp()` cannot be an index bound.
- **`now()` and `CURRENT_TIMESTAMP` never decide an expiry**, because both are fixed at transaction start.
- A time supplied by another system is stored as given in its own column, apart from the received or processed stamp.

## Standard Columns

| Column | Type | Required | Notes |
|--------|------|----------|-------|
| `id` | `UUID PRIMARY KEY` | Yes | UUIDv7, application-minted |
| `created_at` | `TIMESTAMPTZ(3) NOT NULL` | Yes | `now()` at INSERT |
| `updated_at` | `TIMESTAMPTZ(3) NOT NULL` | If the row ever changes | `now()` at INSERT and every UPDATE |

## Audit Pattern

- **Actor tracking:** `actor TEXT` in event and transition tables.
- **No `updated_by` column.** Changes to mutable tables are tracked in event tables, not inline.
- **Append-only enforcement:** an immutable table refuses UPDATE by trigger; DELETE follows grants and the deletion matrix.
  ```sql
  CREATE OR REPLACE FUNCTION {schema}.{table}_refuse_update() RETURNS trigger AS $$
  BEGIN
      RAISE EXCEPTION '{table} is append-only';
  END;
  $$ LANGUAGE plpgsql;

  CREATE TRIGGER trg_{table}_refuse_update
      BEFORE UPDATE ON {schema}.{table}
      FOR EACH ROW EXECUTE FUNCTION {schema}.{table}_refuse_update();
  ```

## Randomness

- Identifiers and secrets come from `afd_core::id` and the operating system's cryptographic source; never a custom random number generator.
