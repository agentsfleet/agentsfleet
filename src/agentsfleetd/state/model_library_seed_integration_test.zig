//! Integration proof that `make seed-models` produces a catalogue the product can
//! actually use — not merely rows in a table.
//!
//! The catalogue is a hard gate, not a display cache: `capFor` backs both the
//! platform-default guard (ERR_PROVIDER_MODEL_NOT_IN_CATALOGUE) and self-managed
//! activation for named providers (UZ-PROVIDER-004). On an unseeded install the
//! only posture a tenant can activate is `openai-compatible`. So "did the seed
//! work" is really "does capFor now answer for the combinations we ship", which
//! is what this asserts.
//!
//! `make test-integration-seed-models` runs the real script with `--fixtures`
//! against a reset database immediately before this lane, so the committed API
//! snapshots exercise the live field-mapping and per-token conversion paths
//! without the suite going red because Pioneer had a bad afternoon.
//!
//! Absent a database this skips. With one, an EMPTY catalogue is a hard failure,
//! not a skip: it means the seed step did not run, and a silent skip there would
//! turn a broken make wiring into a green suite.

const std = @import("std");
const testing = std.testing;
const env = @import("common").env;
const base = @import("../db/test_fixtures.zig");
const store = @import("model_library_store.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

/// Kishore's five subscriptions — the combinations that must be usable as a
/// platform default the moment the seed lands. Each is (provider, model_id).
const SUBSCRIBED_COMBOS = [_][2][]const u8{
    .{ "pioneer", "claude-sonnet-5" }, // Claude without an Anthropic account
    .{ "fireworks", "accounts/fireworks/models/glm-5p2" }, // slashed vendor path
    .{ "openrouter", "anthropic/claude-opus-4.8" }, // slashed id + per-token rates
    .{ "kimi", "kimi-k3" }, // the live setup
    .{ "glm", "glm-5.2" }, // renamed from `zai` to match nullclaw's general endpoint
};

/// Per-provider row counts the allowlist ships — the drift detector: adding or
/// removing a model without noticing these is exactly the accident worth
/// catching. Asserted per provider, NOT as one summed count, because this
/// binary's other suites legitimately insert under both suite-private names
/// (test_fireworks, m100fw, ratecache-probe — invisible here by construction)
/// and one REAL name: secrets_json_integration_test seeds (anthropic,
/// claude-sonnet-4-6) and keeps it. A single scoped total absorbs that row and
/// flakes on import order — the failure Fable's review predicted and both lane
/// runs hit ("expected 77, found 78"). anthropic is therefore floor-asserted;
/// every other provider is owned solely by the seed and pinned exactly.
const PROVIDER_COUNTS = [_]struct { provider: []const u8, rows: i64, exact: bool }{
    .{ .provider = "anthropic", .rows = 4, .exact = false }, // + other suites' rows
    .{ .provider = "openai", .rows = 5, .exact = true },
    .{ .provider = "gemini", .rows = 3, .exact = true },
    .{ .provider = "deepseek", .rows = 2, .exact = true },
    .{ .provider = "kimi", .rows = 3, .exact = true },
    .{ .provider = "glm", .rows = 1, .exact = true },
    .{ .provider = "minimax", .rows = 1, .exact = true },
    .{ .provider = "qwen", .rows = 1, .exact = true },
    .{ .provider = "xai", .rows = 2, .exact = true },
    .{ .provider = "groq", .rows = 2, .exact = true },
    .{ .provider = "mistral", .rows = 5, .exact = true },
    .{ .provider = "pioneer", .rows = 19, .exact = true },
    .{ .provider = "openrouter", .rows = 11, .exact = true },
    .{ .provider = "fireworks", .rows = 7, .exact = true },
    .{ .provider = "together-ai", .rows = 5, .exact = true },
    .{ .provider = "novita", .rows = 6, .exact = true },
};

const PROVIDER_COUNT_SQL = "SELECT COUNT(*)::bigint FROM core.model_library WHERE provider = $1";
const RATE_SQL =
    \\SELECT input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok
    \\FROM core.model_library WHERE provider = $1 AND model_id = $2
;

/// The test DB URL, or null when neither var is set (suite skips).
fn testDbUrl() ?[]const u8 {
    return env.testLiveValue("TEST_DATABASE_URL") orelse env.testLiveValue("DATABASE_URL");
}

/// One provider's live row count. Helper-scoped so the PgQuery drains on return
/// — an inline query with a deferred deinit holds the connection busy for the
/// next statement (pg's ConnectionBusy), which is the drain footgun PgQuery
/// exists to prevent.
fn providerRowCount(conn: anytype, provider: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(PROVIDER_COUNT_SQL, .{provider}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.NoCountRow;
    return row.get(i64, 0);
}

/// Every allowlisted provider carries its expected rows. An entirely empty
/// catalogue is a hard failure, not a skip: it means the make seed step did not
/// run, and a silent skip there would turn broken wiring into a green suite.
fn expectSeededCounts(conn: anytype) !void {
    var total: i64 = 0;
    for (PROVIDER_COUNTS) |expected| {
        const got = try providerRowCount(conn, expected.provider);
        total += got;
        const ok = if (expected.exact) got == expected.rows else got >= expected.rows;
        if (!ok) {
            std.debug.print(
                "provider {s}: expected {s}{d} rows, found {d}\n",
                .{ expected.provider, if (expected.exact) "" else ">=", expected.rows, got },
            );
            return error.SeededCountMismatch;
        }
    }
    if (total == 0) return error.CatalogueNotSeeded;
}

fn rateFor(conn: anytype, provider: []const u8, model: []const u8) ?[3]i64 {
    var q = PgQuery.from(conn.query(RATE_SQL, .{ provider, model }) catch return null);
    defer q.deinit();
    const row = (q.next() catch return null) orelse return null;
    return .{
        row.get(i64, 0) catch return null,
        row.get(i64, 1) catch return null,
        row.get(i64, 2) catch return null,
    };
}

test "integration(seed-models): seeds a catalogue the platform-default guard accepts" {
    if (testDbUrl() == null) return error.SkipZigTest;

    const db_ctx = (try base.openTestConn(testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    // Loud, not skipped, when nothing landed — see the header note on a broken
    // make wiring — then per-provider drift detection.
    try expectSeededCounts(conn);

    // The gate the platform default actually consults. A non-null cap here is
    // precisely what turns ERR_PROVIDER_MODEL_NOT_IN_CATALOGUE into a working ★.
    for (SUBSCRIBED_COMBOS) |combo| {
        const cap = store.capFor(conn, combo[0], combo[1]) orelse {
            std.debug.print("no catalogue cap for {s}/{s}\n", .{ combo[0], combo[1] });
            return error.CombinationNotSeeded;
        };
        try testing.expect(cap > 0);
    }

    // The negative half: without it this suite would still pass if the guard were
    // deleted outright, which would prove the opposite of what it claims.
    try testing.expect(store.capFor(conn, "anthropic", "not-a-real-model") == null);
    try testing.expect(store.capFor(conn, "not-a-real-provider", "claude-sonnet-5") == null);
}

test "integration(seed-models): rates land in nanos, including the per-token source" {
    if (testDbUrl() == null) return error.SkipZigTest;

    const db_ctx = (try base.openTestConn(testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    // Manual source: allowlist USD per Mtok -> nanos. $3.00 / $0.30 / $15.00.
    // Load-bearing beyond units: the make lane deliberately corrupts THIS row's
    // input rate between its two seed passes, so 3_000_000_000 here proves the
    // refresh's ON CONFLICT DO UPDATE actually corrected drift — a re-seed that
    // only ever INSERTs (or diffs to nothing) leaves the planted 1 behind.
    const kimi = rateFor(conn, "kimi", "kimi-k3") orelse return error.KimiRowMissing;
    try testing.expectEqual(@as(i64, 3_000_000_000), kimi[0]);
    try testing.expectEqual(@as(i64, 300_000_000), kimi[1]);
    try testing.expectEqual(@as(i64, 15_000_000_000), kimi[2]);

    // API source, per-TOKEN: OpenRouter publishes 0.000005 USD/token for input.
    // Landing 5_000_000_000 proves the x1e6 -> x1e9 chain, the field mapping, and
    // the fixture parse all held. A missed conversion here would be a 1,000,000x
    // billing error, so it is pinned exactly rather than by magnitude.
    const or_opus = rateFor(conn, "openrouter", "anthropic/claude-opus-4.8") orelse
        return error.OpenRouterRowMissing;
    try testing.expectEqual(@as(i64, 5_000_000_000), or_opus[0]);
    try testing.expectEqual(@as(i64, 500_000_000), or_opus[1]);
    try testing.expectEqual(@as(i64, 25_000_000_000), or_opus[2]);
}

test "integration(seed-models): re-running is idempotent, not duplicative" {
    if (testDbUrl() == null) return error.SkipZigTest;

    const db_ctx = (try base.openTestConn(testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    // The operator runs this monthly, so a second pass must correct drift without
    // duplicating rows or tripping the (provider, model_id) unique key. The make
    // target seeds, corrupts one rate, and seeds again before this lane; counts
    // still matching the allowlist proves the refresh duplicated nothing.
    try expectSeededCounts(conn);
}
