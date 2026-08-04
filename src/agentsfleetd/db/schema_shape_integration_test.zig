//! Catalogue-wide shape assertions against the migrated database.
//!
//! The per-table schema tests live in `fleet/schema_migration_test.zig`. These
//! back the claims that are *universal* — "no table…", "every foreign key…" —
//! which a per-table assertion cannot establish however many tables it names.
//!
//! They read `pg_catalog` and `information_schema` rather than the SQL sources,
//! so a slot that emits the right text but installs the wrong object still
//! fails. Every one asserts something non-vacuous first: a catalogue that failed
//! to create its constraints would otherwise satisfy a "count the violations"
//! assertion by having nothing to count.
//!
//! This file belongs to the integration root because that is the only binary
//! the lanes hand a live `TEST_DATABASE_URL`. The same tests placed beside the
//! unit suites would skip in every gate and prove nothing.

const std = @import("std");
const pg = @import("pg");
const base = @import("test_fixtures.zig");
const PgQuery = @import("pg_query.zig").PgQuery;

const ALLOC = std.testing.allocator;

/// The schemas this product creates. `public` is Postgres's own and is not this
/// milestone's to justify.
const APP_SCHEMAS = "'audit','billing','core','fleet','memory','vault'";

/// One reference in the catalogue legitimately points at something other than a
/// primary key: the platform provider defaults reference
/// `core.model_library (provider, model_id)`, the declared domain key. Every
/// other reference resolves to a primary key, or to a unique constraint that
/// strictly contains one.
const EXPECTED_DOMAIN_KEY_REFERENCES: i64 = 1;

/// Drain-safe scalar read: the deferred `deinit` would otherwise leave the prior
/// result in flight and the next query on the same connection would raise
/// `error.ConnectionBusy`.
fn scalarI64(conn: *pg.Conn, sql: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(sql, .{}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    return row.get(i64, 0);
}

test "integration: every foreign key resolves to a primary key, a superkey, or the one declared domain key" {
    const ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);

    try std.testing.expect(try scalarI64(ctx.conn,
        \\SELECT count(*)::bigint FROM pg_constraint fk
        \\JOIN pg_class rel ON rel.oid = fk.confrelid
        \\JOIN pg_namespace nsp ON nsp.oid = rel.relnamespace
        \\WHERE fk.contype = 'f' AND nsp.nspname IN (
    ++ APP_SCHEMAS ++ ")") > 0);

    // `pk.conkey <@ fk.confkey` reads: the referenced table's primary-key
    // columns are contained in the columns this reference names. A reference to
    // the primary key passes; so does one to a tenant-scoping superkey that adds
    // a column. A duplicate-identity twin passes neither — which is the shape
    // this milestone removed, and the reason this counts rather than inspects.
    try std.testing.expectEqual(EXPECTED_DOMAIN_KEY_REFERENCES, try scalarI64(ctx.conn,
        \\SELECT count(*)::bigint FROM pg_constraint fk
        \\JOIN pg_class rel ON rel.oid = fk.confrelid
        \\JOIN pg_namespace nsp ON nsp.oid = rel.relnamespace
        \\WHERE fk.contype = 'f' AND nsp.nspname IN (
    ++ APP_SCHEMAS ++
        \\)
        \\  AND NOT EXISTS (
        \\    SELECT 1 FROM pg_constraint pk
        \\    WHERE pk.conrelid = fk.confrelid AND pk.contype = 'p' AND pk.conkey <@ fk.confkey
        \\  )
    ));
}

test "integration: no table carries a second unique key over its own primary key columns" {
    const ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);

    // R2's grep covers the `GENERATED ALWAYS` half of this Dimension at source
    // level; this is the half no grep reaches, because a duplicate key is a
    // RELATIONSHIP between two constraints rather than a spelling.
    //
    // Why it matters is upstream in `schema/650`: `ON CONFLICT` arbitrates
    // exactly one constraint, so two sessions inserting a brand-new row race to
    // a duplicate-key error on the OTHER unique index instead of taking the
    // update arm. Every table carrying the shape has that latent race.
    try std.testing.expect(try scalarI64(ctx.conn,
        \\SELECT count(*)::bigint FROM pg_index i
        \\JOIN pg_class rel ON rel.oid = i.indrelid
        \\JOIN pg_namespace nsp ON nsp.oid = rel.relnamespace
        \\WHERE i.indisunique AND NOT i.indisprimary AND nsp.nspname IN (
    ++ APP_SCHEMAS ++ ")") > 0);

    // Unique INDEXES, not just unique constraints: a bare `CREATE UNIQUE INDEX`
    // carries the identical race and appears nowhere in `pg_constraint`.
    // Set equality both ways — a superkey that ADDS a column is the legitimate
    // tenant-scoping shape and must pass.
    try std.testing.expectEqual(@as(i64, 0), try scalarI64(ctx.conn,
        \\SELECT count(*)::bigint FROM pg_index i
        \\JOIN pg_class rel ON rel.oid = i.indrelid
        \\JOIN pg_namespace nsp ON nsp.oid = rel.relnamespace
        \\WHERE i.indisunique AND NOT i.indisprimary AND nsp.nspname IN (
    ++ APP_SCHEMAS ++
        \\)
        \\  AND EXISTS (
        \\    SELECT 1 FROM pg_index pk
        \\    WHERE pk.indrelid = i.indrelid AND pk.indisprimary
        \\      AND (SELECT array_agg(k ORDER BY k) FROM unnest(pk.indkey) k)
        \\        = (SELECT array_agg(k ORDER BY k) FROM unnest(i.indkey) k)
        \\  )
    ));

    // The identity half, asserted against the catalogue rather than the source
    // text so a column added outside `schema/` is caught too.
    try std.testing.expectEqual(@as(i64, 0), try scalarI64(ctx.conn,
        \\SELECT count(*)::bigint FROM pg_attribute a
        \\JOIN pg_class rel ON rel.oid = a.attrelid
        \\JOIN pg_namespace nsp ON nsp.oid = rel.relnamespace
        \\WHERE nsp.nspname IN (
    ++ APP_SCHEMAS ++
        \\) AND rel.relkind IN ('r','p') AND a.attidentity <> ''
    ));
}

test "integration: the ledger resolves every identity through a typed foreign key" {
    const ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);

    // Three references, one per identity. Before the rebuild `tenant_id` was a
    // UUID with no reference and the other two were bare TEXT — which is why
    // erasing an account needed a hand-maintained delete order and why the
    // counter trigger regex-checked its own `fleet_id` before casting.
    try std.testing.expectEqual(@as(i64, 3), try scalarI64(ctx.conn,
        \\SELECT count(*)::bigint FROM pg_constraint fk
        \\JOIN pg_class rel ON rel.oid = fk.conrelid
        \\JOIN pg_namespace nsp ON nsp.oid = rel.relnamespace
        \\WHERE fk.contype = 'f' AND nsp.nspname = 'billing'
        \\  AND rel.relname = 'usage_ledger'
    ));

    try std.testing.expectEqual(@as(i64, 3), try scalarI64(ctx.conn,
        \\SELECT count(*)::bigint FROM information_schema.columns
        \\WHERE table_schema = 'billing' AND table_name = 'usage_ledger'
        \\  AND column_name IN ('tenant_id','workspace_id','fleet_id')
        \\  AND data_type = 'uuid'
    ));

    // The three delete DIFFERENTLY, and that is the point rather than an
    // oversight. `tenant_id` cascades so erasure leaves zero rows (Dimension
    // 3.3); the other two SET NULL so an ordinary fleet delete cannot erase a
    // charge already debited and falsify wallet reconciliation. Asserted
    // explicitly so widening either to a cascade is a red test, not a silent
    // hole in the money record.
    try std.testing.expectEqual(@as(i64, 1), try scalarI64(ctx.conn,
        \\SELECT count(*)::bigint FROM pg_constraint fk
        \\JOIN pg_class rel ON rel.oid = fk.conrelid
        \\JOIN pg_namespace nsp ON nsp.oid = rel.relnamespace
        \\WHERE fk.contype = 'f' AND nsp.nspname = 'billing'
        \\  AND rel.relname = 'usage_ledger' AND fk.confdeltype = 'c'
    ));
    try std.testing.expectEqual(@as(i64, 2), try scalarI64(ctx.conn,
        \\SELECT count(*)::bigint FROM pg_constraint fk
        \\JOIN pg_class rel ON rel.oid = fk.conrelid
        \\JOIN pg_namespace nsp ON nsp.oid = rel.relnamespace
        \\WHERE fk.contype = 'f' AND nsp.nspname = 'billing'
        \\  AND rel.relname = 'usage_ledger' AND fk.confdeltype = 'n'
    ));
}

test "integration: the ledger carries the originating event's creation time as a partition-ready key" {
    const ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);

    // A partition key must be NOT NULL to be usable as one, so the constraint
    // is the assertion rather than mere presence. That every row for one event
    // carries the same value — the property that stops a late renewal landing
    // in a different partition and duplicating the row — is behavioural, and
    // lives in `fleet/credit_metric_reconciliation_integration_test.zig`.
    try std.testing.expectEqual(@as(i64, 1), try scalarI64(ctx.conn,
        \\SELECT count(*)::bigint FROM information_schema.columns
        \\WHERE table_schema = 'billing' AND table_name = 'usage_ledger'
        \\  AND column_name = 'event_created_at'
        \\  AND data_type = 'bigint' AND is_nullable = 'NO'
    ));
}

test "integration: no schema is created that holds no table" {
    const ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);

    // `relkind IN ('r','p')` counts ordinary and partitioned tables. A schema
    // holding only a view or a function is still empty for this purpose:
    // nothing durable lives there, so nothing justifies the grant surface.
    try std.testing.expectEqual(@as(i64, 0), try scalarI64(ctx.conn,
        \\SELECT count(*)::bigint FROM pg_namespace n
        \\WHERE n.nspname IN (
    ++ APP_SCHEMAS ++
        \\)
        \\  AND NOT EXISTS (
        \\    SELECT 1 FROM pg_class c WHERE c.relnamespace = n.oid AND c.relkind IN ('r','p')
        \\  )
    ));
}

test "integration: row lifecycle timestamps carry one naming form across every table" {
    const ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);

    // The rebuild settled on `created_at` / `updated_at`. The `_at_ms` spelling
    // it replaced is what would return by copy-paste from a retired slot, so
    // that is the spelling this refuses.
    try std.testing.expectEqual(@as(i64, 0), try scalarI64(ctx.conn,
        \\SELECT count(*)::bigint FROM information_schema.columns
        \\WHERE table_schema IN (
    ++ APP_SCHEMAS ++
        \\)
        \\  AND column_name LIKE '%\_at\_ms'
    ));
}

test "integration: no trigger body pattern-matches, because its input is no longer text" {
    const ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer ctx.pool.deinit();
    defer ctx.pool.release(ctx.conn);

    try std.testing.expect(try scalarI64(ctx.conn,
        \\SELECT count(*)::bigint FROM pg_proc p
        \\JOIN pg_namespace n ON n.oid = p.pronamespace
        \\WHERE n.nspname IN (
    ++ APP_SCHEMAS ++
        \\) AND EXISTS (SELECT 1 FROM pg_trigger t WHERE t.tgfoid = p.oid)
    ) > 0);

    // A regular expression in a trigger body means the trigger is parsing text
    // that should have arrived typed. The counter trigger ran one on every
    // renewal, because the identifiers it matched were stored as text.
    try std.testing.expectEqual(@as(i64, 0), try scalarI64(ctx.conn,
        \\SELECT count(*)::bigint FROM pg_proc p
        \\JOIN pg_namespace n ON n.oid = p.pronamespace
        \\WHERE n.nspname IN (
    ++ APP_SCHEMAS ++
        \\) AND EXISTS (SELECT 1 FROM pg_trigger t WHERE t.tgfoid = p.oid)
        \\  AND (p.prosrc LIKE '%~%' OR upper(p.prosrc) LIKE '%SIMILAR TO%')
    ));
}
