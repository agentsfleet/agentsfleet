# SQL and schema

Rules for Postgres schema slots (one numbered schema file each), migrations, grants, and the SQL that Rust code runs at request time. A coding agent applies them before editing a slot, the migration list or a statement file. A reviewer judges a database diff against them.

## Migrations and live data

### SCH — A schema change is a new forward slot, and shipped slots stay frozen as history.
- **Applies to:** `**/migrations/*.sql`, the repository's migration list
- **Checked by:** review
- **Bad:** editing the shipped `migrations/1500_planning_projects.sql` to drop a column, or deleting a shipped slot
- **Good:** a new numbered slot holding `ALTER TABLE planning.projects DROP COLUMN …`, with the owner's approval for that drop recorded
- **Why:** a live datastore holds data nobody can re-create, and the migrator refuses an applied slot whose checksum changed.

A destructive change (`DROP TABLE`, `DROP COLUMN`, a type rewrite) and any from-empty rebuild each need the owner's approval for that change.
Another product's version number never proves this datastore is disposable, and shipped slot numbers are never reused or slid.

### MIG — The migration list and every position-based assertion change in the same diff as the slots they track.
- **Applies to:** the repository's migration list and the tests that assert slot positions
- **Checked by:** review
- **Bad:** splitting a slot while a test still asserts that `migrations[7]` names the old file
- **Good:** the split registers both slots and moves every index assertion to the new positions
- **Why:** a stale position points at the wrong SQL file without a compile error.

### SQL-SLOT — One slot holds one table with its constraints, indexes and grants, in at most 100 lines.
- **Applies to:** `**/migrations/*.sql`
- **Checked by:** review
- **Bad:** one slot creating `planning.projects` and `planning.revisions`, with their grants collected in a later file
- **Good:** `1500_planning_projects.sql` and `1510_planning_revisions.sql`, each ending with its own `GRANT`
- **Why:** a table, its indexes and its grants are read, reviewed and changed together.

### SQL-ORDER — Slots run in foreign-key order across schemas, and a foreign key that closes a cycle gets its own later slot.
- **Applies to:** `**/migrations/*.sql` and the migration list
- **Checked by:** review
- **Bad:** `planning.projects` declaring `head_revision_id … REFERENCES planning.revisions(id)` before `planning.revisions` exists
- **Good:** `1520_planning_projects_head_revision.sql` adds that key after both tables, `DEFERRABLE INITIALLY DEFERRED`
- **Why:** a slot cannot reference a table that does not exist yet, and a deferred key lets both rows land in one transaction.

### SQL-MIGRATOR — Migrations run through SQLx's `Migrator` over the explicit slot list, with checksums and a bounded advisory lock.
- **Applies to:** the migration runner module and the repository's migration list
- **Checked by:** review
- **Bad:** the `migrate!` macro over a directory, or SQLx's own lock waiting without a bound
- **Good:** `Migrator::with_migrations(list)` with `set_locking(false)` inside a bounded `pg_try_advisory_lock` poll, and the `macros` feature off
- **Why:** a held lock then fails a deploy loudly instead of hanging it, and an edited applied slot is refused.

### SGR — Every `CREATE TABLE` grants exactly the operations its runtime callers need, to the roles that make those calls.
- **Applies to:** `**/migrations/*.sql`
- **Checked by:** review
- **Bad:** a new table with no `GRANT`, on the belief that production connects as a superuser
- **Good:** `GRANT SELECT, INSERT, DELETE ON identity.workspaces TO api_runtime;` at the end of the table's own slot
- **Why:** Postgres denies by default, so a missing grant fails only at the first runtime query.

Role names come from the repository's connection configuration, never from another product.

## Literals and constants

### STS — No string literal in a schema slot names a value or identifier the application also names.
- **Applies to:** `**/migrations/*.sql`
- **Checked by:** review
- **Bad:** `DEFAULT 'active'`, `CHECK (status IN ('pending', 'approved'))`, `OLD.status != 'pending'` in a trigger, `current_setting('app.allow_purge')`
- **Good:** values bound from application constants, a predicate on a NULL marker column, and a privileged path granted to a role
- **Why:** SQL cannot import an application constant, and a drifted trigger predicate or setting name stops guarding without failing.

`RAISE EXCEPTION` text a caller matches on counts too, and where SQL must spell an application name, such as a role, a test pins the pair.
Structural literals nothing in the application names stay: `'{}'::jsonb`, the Universally Unique Identifier (UUID) version nibble `'7'`, `TG_OP` values.

### NSQ — SQL names every table with its schema and holds no magic numbers.
- **Applies to:** `**/migrations/*.sql`, statement files (`**/src/**/*.sql`)
- **Checked by:** review
- **Bad:** `SELECT key_name FROM secrets WHERE workspace_id = $1 LIMIT 100`
- **Good:** `SELECT key_name FROM vault.secrets WHERE workspace_id = $1 LIMIT $2`, with the limit bound from a named constant
- **Why:** an unqualified name resolves through `search_path`, which differs across environments and roles.

### SQL-DEFAULT — A numeric schema default that runtime code also uses is one Rust named constant, cited in the slot.
- **Applies to:** `**/migrations/*.sql` and the crate that owns the default
- **Checked by:** review
- **Bad:** `DEFAULT 5` in a slot and a separate literal `5` in a Rust fallback
- **Good:** `-- Canonical constant: crate::path::NAME` beside the column, whose value changes only when the product default changes
- **Why:** two spellings of one default drift apart with no failing test.

## Tables and columns

### SQL-PLACE — A table lives in the schema of the product area that owns it, named as a plural noun that does not repeat the schema.
- **Applies to:** `CREATE TABLE` in `**/migrations/*.sql`
- **Checked by:** review
- **Bad:** `authoring.authoring_drafts`, or a draft table placed in the `planning` schema
- **Good:** `authoring.drafts`
- **Why:** the schema already names the owning area, so the table name need not.

A singleton table is named for its one row and says so in its slot.

### SQL-FKNAME — A foreign-key column ends `_id`, names the table it points at, and adds a role prefix when the name alone is unclear.
- **Applies to:** `**/migrations/*.sql`
- **Checked by:** review
- **Bad:** `owner UUID REFERENCES identity.users(id)`
- **Good:** `user_id`, `draft_id`, `invited_by_user_id`, `last_runner_id`
- **Why:** the column name then says what it points at and in which role.

A deliberately missing foreign key is explained in the slot comment.

### SQL-STATUS — A lifecycle column is named `status`, and a category column is named `<thing>_type`.
- **Applies to:** `**/migrations/*.sql`
- **Checked by:** review
- **Bad:** `state` for a lifecycle, `category` for a classification
- **Good:** `status`, `event_type`, `gate_type`
- **Why:** one spelling per concept keeps statements, wire types and reviews aligned.

### SQL-COUNTER — Positions, watermarks, versions and tallies use the canonical column names.
- **Applies to:** `**/migrations/*.sql`
- **Checked by:** review
- **Bad:** `seq_no`, `max_seq`, `version` for a compare-and-swap counter, `epoch` for a lease
- **Good:** `sequence`, `<thing>_sequence`, `last_<thing>_sequence`, `<verb>_through_sequence`, `resource_version`, `fencing_token`, `<thing>_count`
- **Why:** a reader tells a position from a watermark or a version by the name alone.

Frozen content is a `revision`, pointed at by `<x>_revision_id`, and the live one is `head_revision_id`.
A one-action edit counter is `<scope>_version`, and a deadline ends `expires_at`. Wire field names keep their published spelling.

### SQL-TIMECOL — Every table has `created_at`, a table whose rows change has `updated_at`, and any other time column has a stated kind.
- **Applies to:** `**/migrations/*.sql`
- **Checked by:** review
- **Bad:** `last_seen_at` that nothing reads, or `updated_at` on an append-only table
- **Good:** a deadline `*expires_at`, a NULL marker an index or trigger reads, an API instant `updated_at` cannot carry, or a billing cursor
- **Why:** every time column then has a reader and one meaning.

A time supplied by another system is stored as given in its own column, apart from the received or processed stamp.

### SQL-READER — Every new column names its reader, and every new table says why it is not a column or an existing table.
- **Applies to:** `**/migrations/*.sql`
- **Checked by:** review
- **Bad:** a `notes TEXT` column that no statement reads
- **Good:** a slot comment naming the statement or export that reads each new column
- **Why:** a column nobody reads is storage and erasure work with no product behind it.

Accounting and audit records are exempt when the slot names the export or verification that reads them.

### SQL-ONEFACT — Each fact has one home: no JSON blob copies a column, and no counter restates another table's number.
- **Applies to:** `**/migrations/*.sql`
- **Checked by:** review
- **Bad:** `projects.config JSONB` repeating `projects.name`, or a `workspaces.project_count` kept by application code
- **Good:** the fact in one column, read through a join
- **Why:** two copies drift, and nothing says which one is true.

Named exceptions state their consistency rule in the slot. They are an aggregate kept under its source's lock or trigger, and a delivery snapshot that outlives its source.
A non-secret projection of encrypted content and a scope copy a restricted role filters on are the others.

### SQL-RETRYKEY — Retry keys, an `operation_id` with a request digest, live only in receipt tables.
- **Applies to:** `**/migrations/*.sql`
- **Checked by:** review
- **Bad:** an `operation_id` column on `authoring.messages`
- **Good:** `authoring.replay_receipts` keyed by `(scope, operation_id)` and holding `request_digest`, with no foreign keys
- **Why:** a retry key must outlive the row it created, or a late retry would create that row again.

### SQL-IMMUTABLE — An immutable table has no `updated_at`, no `UPDATE` grant, and a trigger that refuses `UPDATE`.
- **Applies to:** append-only tables in `**/migrations/*.sql`
- **Checked by:** review
- **Bad:** an append-only table carrying `updated_at`, or granting `UPDATE` to `api_runtime`
- **Good:** `CREATE TRIGGER trg_workspaces_refuse_update BEFORE UPDATE ON identity.workspaces FOR EACH ROW EXECUTE FUNCTION identity.workspaces_refuse_update();`
- **Why:** the trigger refuses an update even from a role that was granted one by mistake.

`DELETE` stays governed by grants and the deletion matrix, so cascades keep working.

### SQL-ACTOR — Event and transition tables record `actor TEXT`, and no mutable table carries `updated_by`.
- **Applies to:** `**/migrations/*.sql`
- **Checked by:** review
- **Bad:** `projects.updated_by UUID`
- **Good:** `planning.events.actor` recording who made each change
- **Why:** an inline `updated_by` keeps only the last writer, while event rows keep every change.

### SQL-ERASE — Every root a user or operator can delete has a stated row in the product's deletion matrix.
- **Applies to:** `ON DELETE` clauses and erasure of users, workspaces, tenants and the records they own
- **Checked by:** review
- **Bad:** a charge row declared `ON DELETE CASCADE` from the workspace that owes it
- **Good:** money records keep opaque identifiers and never cascade from an erasable owner, and private text is scrubbed on every erasure path
- **Why:** an erasure must say what goes with the root, what stays and what is scrubbed at once.

### VLT — An entity table stores a vault key name, never the secret.
- **Applies to:** `**/migrations/*.sql`, entity structs and the statements that write them
- **Checked by:** review
- **Bad:** a `webhook_secret TEXT` column on `planning.projects`
- **Good:** `webhook_secret_ref` holding a `key_name`, opened at runtime with `Vault::open` from the encrypted `vault.secrets`
- **Why:** a plaintext column shows up in query results, backups and logs.

## Identity and time

### SQL-ID — A table has one identity column, `id`, holding a UUID version 7 (UUIDv7) the application minted.
- **Applies to:** `CREATE TABLE` in `**/migrations/*.sql`, and the Rust that inserts rows
- **Checked by:** review
- **Bad:** `id UUID PRIMARY KEY DEFAULT gen_random_uuid()`, or a `BIGSERIAL` key
- **Good:** `id UUID PRIMARY KEY` with `CONSTRAINT ck_workspaces_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7')`, minted by `app_core::id`
- **Why:** the owner chose one identity shape (Jul 31, 2026), and the check refuses any other UUID version.

Exceptions are stated in their slot. A row that exists once per parent is keyed by the parent, as `planning.settings` is.
A pinned-integer singleton and a natural key every statement addresses (`model.platform_defaults` by `provider`) are the others.

### SQL-ONEKEY — A value has one unique key, and a foreign key references the primary key.
- **Applies to:** `**/migrations/*.sql`
- **Checked by:** review
- **Bad:** `id UUID PRIMARY KEY` beside `UNIQUE (public_id)` over the same identity
- **Good:** one key on `id`, referenced as `REFERENCES planning.projects(id)`
- **Why:** `ON CONFLICT` arbitrates one constraint, so concurrent inserts race to a duplicate-key error on the other index.

A composite `UNIQUE (id, tenant_id)` that a reference uses to prove tenant ownership is the stated exception, as in `identity.workspaces`.

### SQL-TSTYPE — A time column is `TIMESTAMPTZ(3)`, nullable only when it is a NULL marker.
- **Applies to:** `**/migrations/*.sql`
- **Checked by:** review
- **Bad:** `created_at BIGINT` holding epoch milliseconds, or `TIMESTAMP` without a time zone
- **Good:** `created_at TIMESTAMPTZ(3) NOT NULL`
- **Why:** the database, `jiff::Timestamp` and the API then agree on millisecond precision.

### SQL-STAMP — The writing statement sets `created_at` and `updated_at` to `now()`, and neither column has a `DEFAULT`.
- **Applies to:** `**/migrations/*.sql`, statement files (`**/src/**/*.sql`)
- **Checked by:** review
- **Bad:** `created_at TIMESTAMPTZ(3) NOT NULL DEFAULT now()`, or a stamp bound from the Rust clock
- **Good:** `INSERT INTO identity.workspaces (id, tenant_id, created_at) VALUES ($1, $2, now())`
- **Why:** one database clock gives one instant per transaction, and every write names its stamp in its file.

## Statements in Rust

### SQL-FILE — Every static statement is a `.sql` file beside the use case that runs it, bound by one typed Rust function.
- **Applies to:** statement files (`**/src/**/*.sql`) and the Rust that loads them
- **Checked by:** review
- **Bad:** `sqlx::query("SELECT … FROM authoring.library_entries …")` spelled inline in a handler
- **Good:** `library/sql/list.sql` loaded with `include_str!` by a function whose signature states its parameters, authorization and locks
- **Why:** a statement in a file is reviewable SQL, and the typed function is the one way to run it.

A statement that must be assembled at run time is named in its module and covered by its own test.

### SQL-ROW — Rows decode into named `FromRow` types through explicit column lists, kept apart from wire types.
- **Applies to:** Rust stores and statement files (`**/src/**/*.sql`)
- **Checked by:** review
- **Bad:** `SELECT *`, `SELECT to_jsonb(p) FROM planning.projects p`, or a storage row serialized straight to the API
- **Good:** `SELECT id, name, created_at FROM planning.projects …` into `ProjectRow`, then mapped into a wire type
- **Why:** an explicit list fails at `PREPARE` when a column moves, and a separate wire type keeps storage changes off the API.

`Json<T>` decodes real JSON columns only.

### SQL-PREPARE — Every runtime `.sql` file is prepared against the rebuilt schema by one integration test.
- **Applies to:** statement files (`**/src/**/*.sql`) and statements joined at compile time
- **Checked by:** gate: verify.integration
- **Bad:** a statement naming a dropped column that fails only on its first request
- **Good:** the test finds every `.sql` file on disk and runs `PREPARE` on it, and names each joined statement in its list
- **Why:** a stale table or column then fails before merge instead of in production.

### SQL-REPO — A domain store takes the caller's connection or transaction, and no generic repository trait exists.
- **Applies to:** store modules in `**/*.rs`
- **Checked by:** review
- **Bad:** `trait Repository<T> { async fn find(&self, id: Uuid) -> Result<T>; }` implemented over a pool
- **Good:** `pub async fn list(conn: &mut PgConnection, workspace_id: Uuid7) -> Result<Vec<ProjectRow>>`
- **Why:** the caller owns the transaction, its locks and its connection, and a generic trait hides all three.

## Connections and transactions

### CNX — A request holds at most one pool connection, and every helper takes the one it holds.
- **Applies to:** handlers, services and stores in `**/*.rs`
- **Checked by:** review
- **Bad:** a service holding `conn` from `pool.acquire()` calls `events::list(&pool, …)`, which acquires a second
- **Good:** `events::list(&mut conn, …)`, with a pool entry point that acquires once and delegates
- **Why:** two connections per request double pool pressure and deadlock a saturated pool.

Releasing the first connection early is no fix, because it drops the session settings the earlier check set.

### SQL-TIMEOUT — Runtime statements run under `lock_timeout` and `statement_timeout` from named constants, and migrations run under neither.
- **Applies to:** pool setup and runtime transactions in `**/*.rs`
- **Checked by:** review
- **Bad:** a runtime transaction with no timeouts, or `statement_timeout` set on the migrate connection
- **Good:** `SELECT set_config('lock_timeout', $1, true), set_config('statement_timeout', $2, true)` bound from `LOCK_TIMEOUT` and `STATEMENT_TIMEOUT`
- **Why:** a stalled lock holder then fails one statement with `55P03` instead of stalling every caller behind it.

### TXN — A transaction with several writes rolls back when any statement in it fails.
- **Applies to:** multi-statement transactions in `**/*.rs`
- **Checked by:** review
- **Bad:** `if let Err(err) = delete_preference.execute(&mut *tx).await { warn!(…) }`, then `tx.commit()`
- **Good:** `delete_preference.execute(&mut *tx).await?;`, so the uncommitted transaction rolls back when dropped
- **Why:** a logged-and-skipped failure commits half the change, such as a deleted key whose preference still points at it.

### IDMP — An idempotency check reopens a terminal row instead of returning its stale status.
- **Applies to:** request flows guarded by a unique key, such as integration grants
- **Checked by:** review
- **Bad:** returning `{"status": "revoked"}` for every existing `(project_id, service)` row
- **Good:** a `revoked` or `denied` row moves back to `pending` by `UPDATE`, and a `pending` or `approved` row returns as it is
- **Why:** the unique key blocks a second insert, so a terminal row would refuse every later request forever.

### SQL-UPSERT — An upsert whose caller needs the row back uses an unconditional no-op `DO UPDATE`.
- **Applies to:** `INSERT … ON CONFLICT` statements
- **Checked by:** review
- **Bad:** `ON CONFLICT DO NOTHING RETURNING id`, which returns no row when a concurrent insert wins
- **Good:** `ON CONFLICT (k) DO UPDATE SET k = t.k RETURNING id, (xmax = 0)`
- **Why:** `DO NOTHING` returns nothing for a conflicting row, so a race reads as a missing row.

### SQL-FKSTATE — A foreign-key refusal is matched by SQLSTATE `23503` or `23001` together with the constraint name.
- **Applies to:** error mapping in Rust stores
- **Checked by:** review
- **Bad:** `is_foreign_key_violation()` alone, which reads `23503` only and ignores which key refused
- **Good:** `app_db::constraint::violates_foreign_key(&error, FK_NAME)`, matching either code and the name
- **Why:** an `ON DELETE RESTRICT` refusal raises `23001`, and a table with two foreign keys raises the same code for either.

## Cursors and clocks

### KYS — A pagination cursor carries the full sort key and the row `id`, never a bare timestamp.
- **Applies to:** keyset pagination statements and cursor codecs
- **Checked by:** review
- **Bad:** a cursor holding only `created_at`, read as `WHERE created_at > $1`
- **Good:** a cursor of `(created_at, id)`, read as `WHERE (created_at, id) > ($1, $2) ORDER BY created_at, id`
- **Why:** several rows share a millisecond, so a scalar cursor skips them at a page edge.

The cursor keeps the ordering key at the database's precision.

### SQL-DEADLINE — A deadline is computed in SQL from `clock_timestamp()` and a bound interval, and `now()` never decides an expiry.
- **Applies to:** statement files (`**/src/**/*.sql`)
- **Checked by:** review
- **Bad:** `WHERE expires_at > now()`, or `CURRENT_TIMESTAMP + interval '30 seconds'`
- **Good:** `clock_timestamp() + $3::interval`, with the time to live bound as an exact interval
- **Why:** `now()` and `CURRENT_TIMESTAMP` are fixed at transaction start, so a long transaction judges expiry by a stale clock.

### SQL-LOCKCLOCK — A locking validity check locks its rows first, then reads `clock_timestamp()` once in a `MATERIALIZED` step that depends on them.
- **Applies to:** lease, hold and deadline checks that take row locks
- **Checked by:** review
- **Bad:** `SELECT … FROM runner.leases WHERE expires_at > clock_timestamp() FOR UPDATE`
- **Good:** `WITH locked AS (SELECT … FOR UPDATE), clock AS MATERIALIZED (SELECT clock_timestamp() AS at FROM locked LIMIT 1) …`
- **Why:** a clock read in the `WHERE` or select list of a locking statement can run before the lock wait and accept an expired row.

### SQL-SWEEPCLOCK — A non-locking read or a `SKIP LOCKED` sweep compares deadlines with `statement_timestamp()`.
- **Applies to:** sweeps and non-locking reads in statement files (`**/src/**/*.sql`)
- **Checked by:** review
- **Bad:** `WHERE expires_at < clock_timestamp() … FOR UPDATE SKIP LOCKED`
- **Good:** `WHERE expires_at < statement_timestamp() … FOR UPDATE SKIP LOCKED`
- **Why:** an index can use `statement_timestamp()` as a bound, and it cannot use `clock_timestamp()`.

## Proving a statement

### SQL-ROLE — Runtime SQL is proven under the runtime role, never in an owner or superuser session.
- **Applies to:** integration tests and ad-hoc proofs that a runtime statement works
- **Checked by:** review
- **Bad:** proving a statement as the `postgres` superuser, or as a role holding `pg_read_all_data`
- **Good:** `BEGIN; SET LOCAL ROLE api_runtime; <statement>; ROLLBACK;`
- **Why:** owner, superuser and `pg_*_all_data` sessions bypass grants, so a missing `GRANT` passes there.

### SQL-PLAN — A query-plan claim rests on `EXPLAIN (ANALYZE, BUFFERS)` over seeded rows with a realistic spread, run after `ANALYZE`.
- **Applies to:** index changes and plan claims in specs, comments and Pull Request notes
- **Checked by:** review
- **Bad:** `SET enable_seqscan = off;` then `EXPLAIN` on a near-empty table, cited as proof the index is used
- **Good:** realistic seeded rows, `ANALYZE`, then the cited output of `EXPLAIN (ANALYZE, BUFFERS)`
- **Why:** `enable_seqscan = off` proves only that some index exists, not that the planner picks it.

### SQL-STATACT — `pg_stat_activity` is read from a separate autocommit connection.
- **Applies to:** tests and diagnostics that watch sessions, locks or waits
- **Checked by:** review
- **Bad:** reading `pg_stat_activity` twice inside one open transaction to watch a lock wait
- **Good:** a second autocommit connection that polls `pg_stat_activity`
- **Why:** inside an open transaction the view stays frozen at its first read.
