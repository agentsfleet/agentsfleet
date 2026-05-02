//! Process-singleton cache of per-model token rates.
//!
//! Populated at API server boot from core.model_caps; read on the hot path by
//! tenant_billing.computeStageCharge under platform-managed posture. The cache
//! is treated as read-only after boot — concurrent readers hit the StringHashMap
//! without locks. A future refresh path (every hour, per spec) will need to add
//! synchronization; until then any rate change requires an API restart.
//!
//! Tests construct Cache directly via initFromConn so they never touch the
//! process-global; only serve.zig's boot path calls populate() / deinit().

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

pub const ModelRate = struct {
    input_cents_per_mtok: i64,
    output_cents_per_mtok: i64,
    context_cap_tokens: u32,
};

const RatesMap = std.StringHashMapUnmanaged(ModelRate);

const SELECT_RATES =
    \\SELECT model_id, input_cents_per_mtok, output_cents_per_mtok, context_cap_tokens
    \\FROM core.model_caps
;

pub const Cache = struct {
    arena: std.heap.ArenaAllocator,
    rates: RatesMap,

    pub fn initFromConn(alloc: std.mem.Allocator, conn: *pg.Conn) !Cache {
        var arena = std.heap.ArenaAllocator.init(alloc);
        errdefer arena.deinit();
        const arena_alloc = arena.allocator();

        var rates: RatesMap = .{};
        var q = PgQuery.from(try conn.query(SELECT_RATES, .{}));
        defer q.deinit();
        while (try q.next()) |row| {
            const model_id = try arena_alloc.dupe(u8, try row.get([]const u8, 0));
            const in_rate: i64 = @intCast(try row.get(i32, 1));
            const out_rate: i64 = @intCast(try row.get(i32, 2));
            const cap_i32 = try row.get(i32, 3);
            try rates.put(arena_alloc, model_id, .{
                .input_cents_per_mtok = in_rate,
                .output_cents_per_mtok = out_rate,
                .context_cap_tokens = @intCast(@max(cap_i32, 0)),
            });
        }
        return .{ .arena = arena, .rates = rates };
    }

    pub fn deinit(self: *Cache) void {
        self.arena.deinit();
        self.* = undefined;
    }

    pub fn lookup(self: *const Cache, model: []const u8) ?ModelRate {
        return self.rates.get(model);
    }
};

// ── Process-global singleton (initialized at API boot) ─────────────────────

var global: ?Cache = null;

pub fn populate(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    if (global) |*g| g.deinit();
    global = try Cache.initFromConn(alloc, conn);
}

pub fn lookup_model_rate(model: []const u8) ?ModelRate {
    if (global) |*g| return g.lookup(model);
    return null;
}

pub fn deinit() void {
    if (global) |*g| g.deinit();
    global = null;
}
