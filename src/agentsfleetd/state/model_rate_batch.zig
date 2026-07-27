//! Rates for a whole page of `(provider, model)` pairs, in one statement.
//!
//! Separate from `model_rate_cache.zig` on purpose, and the separation is the
//! design rather than a line-count accident (though RULE FLL is what forced the
//! question). That module is BILLING's: it caches, it compares catalogue
//! generations, and `rateAtRevision` fills it on demand at a generation the
//! caller verified. This one is the DISPLAY read, and it deliberately does none
//! of that.
//!
//! Not caching here matters three ways:
//!
//!   - Admitting rows from a display read would fill a billing cache from a path
//!     that observed no revision, so a later charge could accept an entry whose
//!     generation nothing checked.
//!   - It would allocate process-lifetime key strings on a request path — the
//!     hazard `model_rate_cache.backing_allocator`'s note already describes,
//!     from the time the admin handler passed its request arena in.
//!   - It would be a SECOND way to fill one cache. Removing the boot warm was
//!     meant to end exactly that, and two fill paths drift.
//!
//! Living in its own module makes all three structural: there is no cache in
//! scope to accidentally populate.
//!
//! What this replaced on the registry page: a resident-only cache read. It cost
//! no statement, but it answered null for every row until some unrelated billing
//! charge happened to load that exact pair — so after a restart the Models page
//! showed blank rates indefinitely, and nothing filled the cache for display any
//! more once the boot warm and the fixture `populate()` were removed.

const std = @import("std");
const pg = @import("pg");

const PgQuery = @import("../db/pg_query.zig").PgQuery;
const sql = @import("model_library/sql.zig");
const model_rate_cache = @import("model_rate_cache.zig");

/// Re-exported so a caller needs one import, not two, for one concept. The type
/// stays owned by the cache because that is what stores it.
pub const ModelRate = model_rate_cache.ModelRate;

/// Fill `out` with the rate for each `(providers[i], models[i])` pair.
///
/// Positional: `out[i]` belongs to pair `i`, mirroring `vault.loadMetadata`. A
/// pair the catalogue does not carry leaves its slot null, which the registry
/// page renders as a blank cell rather than a wrong number — every rate field on
/// that view is already nullable.
///
/// One statement whatever the page size. That independence is what §3's budget
/// actually pins; a per-row lookup would make the statement count a function of
/// `limit`, which is the unbounded shape the workstream exists to remove.
pub fn loadRatesForPairs(
    conn: *pg.Conn,
    providers: []const []const u8,
    models: []const []const u8,
    out: []?ModelRate,
) !void {
    std.debug.assert(out.len == providers.len);
    std.debug.assert(out.len == models.len);
    @memset(out, null);
    // Nothing to price. The guard is what keeps a degenerate page off the
    // database entirely, and `library_read_bounds_integration_test.zig` pins the
    // resulting statement count so it cannot be dropped unnoticed.
    if (providers.len == 0) return;

    var q = PgQuery.from(try conn.query(sql.LOAD_RATES_FOR_PAIRS, .{ providers, models }));
    defer q.deinit();
    while (try q.next()) |row| {
        const provider = try row.get([]const u8, 0);
        const model = try row.get([]const u8, 1);
        const cap = (try row.get(?i32, 2)) orelse 0;
        const rate: ModelRate = .{
            .context_cap_tokens = @intCast(@max(cap, 0)),
            .input_nanos_per_mtok = (try row.get(?i64, 3)) orelse 0,
            .cached_input_nanos_per_mtok = (try row.get(?i64, 4)) orelse 0,
            .output_nanos_per_mtok = (try row.get(?i64, 5)) orelse 0,
        };
        // EVERY matching slot, not just the first: the caller passes a positional
        // list, and one pair legitimately repeats — the platform default is
        // usually also one of the tenant's own rows. Rates are plain data, so a
        // repeat is a copy rather than a second owner.
        for (providers, models, 0..) |p, m, i| {
            if (out[i] == null and std.mem.eql(u8, p, provider) and std.mem.eql(u8, m, model)) {
                out[i] = rate;
            }
        }
    }
}
