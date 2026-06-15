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
    input_nanos_per_mtok: i64,
    cached_input_nanos_per_mtok: i64,
    output_nanos_per_mtok: i64,
    context_cap_tokens: u32,
};

const RatesMap = std.StringHashMapUnmanaged(ModelRate);

/// Map-key separator joining (provider, model_id) into a single lookup key.
/// ASCII unit-separator — never appears in a provider name or model_id, so it
/// cannot collide a key boundary. The same model_id under two providers
/// (claude-opus-4-8 on anthropic vs pioneer) maps to two distinct keys.
const KEY_SEP: u8 = 0x1f;

const SELECT_RATES =
    \\SELECT provider, model_id, input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok, context_cap_tokens
    \\FROM core.model_caps
;

/// Write the composite (provider, model) lookup key into `buf`. Returns null
/// if the pair does not fit — caller treats that as a cache miss (loud at
/// billing), never a silent wrong-rate.
fn writeKey(buf: []u8, provider: []const u8, model: []const u8) ?[]const u8 {
    if (provider.len + model.len + 1 > buf.len) return null;
    @memcpy(buf[0..provider.len], provider);
    buf[provider.len] = KEY_SEP;
    @memcpy(buf[provider.len + 1 ..][0..model.len], model);
    return buf[0 .. provider.len + 1 + model.len];
}

pub const Cache = struct {
    const Self = @This();

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
            const provider = try row.get([]const u8, 0);
            const model_id = try row.get([]const u8, 1);
            const in_rate = try row.get(i64, 2);
            const cached_rate = try row.get(i64, 3);
            const out_rate = try row.get(i64, 4);
            const cap_i32 = try row.get(i32, 5);
            var key_buf: [512]u8 = undefined;
            const key_src = writeKey(&key_buf, provider, model_id) orelse continue;
            const key = try arena_alloc.dupe(u8, key_src);
            try rates.put(arena_alloc, key, .{
                .input_nanos_per_mtok = in_rate,
                .cached_input_nanos_per_mtok = cached_rate,
                .output_nanos_per_mtok = out_rate,
                .context_cap_tokens = @intCast(@max(cap_i32, 0)),
            });
        }
        return .{ .arena = arena, .rates = rates };
    }

    pub fn deinit(self: *Self) void {
        self.arena.deinit();
        self.* = undefined;
    }

    pub fn lookup(self: *const Self, provider: []const u8, model: []const u8) ?ModelRate {
        var key_buf: [512]u8 = undefined;
        const key = writeKey(&key_buf, provider, model) orelse return null;
        return self.rates.get(key);
    }
};

// ── Process-global singleton (initialized at API boot) ─────────────────────

var global: ?Cache = null;

pub fn populate(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    if (global) |*g| g.deinit();
    global = try Cache.initFromConn(alloc, conn);
}

pub fn lookup_model_rate(provider: []const u8, model: []const u8) ?ModelRate {
    if (global) |*g| return g.lookup(provider, model);
    return null;
}

pub fn deinit() void {
    if (global) |*g| g.deinit();
    global = null;
}
