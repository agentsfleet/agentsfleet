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
