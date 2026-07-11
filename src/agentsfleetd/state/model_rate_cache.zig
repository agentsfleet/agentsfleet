//! Process-singleton cache of per-model token rates.
//!
//! Populated at API server boot from core.model_library; read on the hot path by
//! tenant_billing.computeStageCharge under platform-managed posture. The admin
//! model-caps CRUD API calls populate() again after every mutation so a rate
//! change is live with no restart.
//!
//! Concurrency: the process-global is guarded by a mutex. Hot-path readers
//! (lookup_model_rate) take the lock and copy the ModelRate value out (the
//! struct holds no pointers into the map), so the lock releases the moment the
//! lookup returns — lookups never alias map memory across the unlock. populate()
//! builds the fresh Cache OUTSIDE the lock — the DB query never blocks readers —
//! then takes the lock only to swap the pointer and free the old arena. A failed
//! rebuild leaves the live cache untouched (build-then-swap, never
//! deinit-then-build).
//!
//! Tests construct Cache directly via initFromConn so they never touch the
//! process-global; only serve.zig's boot path and the admin CRUD handler call
//! populate() / deinit().

const std = @import("std");
const pg = @import("pg");
const common = @import("common");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const model_library_store = @import("model_library_store.zig");

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
    "SELECT provider, model_id, input_nanos_per_mtok, cached_input_nanos_per_mtok, output_nanos_per_mtok, context_cap_tokens" ++
    "\nFROM " ++ model_library_store.TABLE;

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
var global_lock: common.Mutex = .{};

/// (Re)build the rate cache from core.model_library. Safe to call at runtime under
/// concurrent readers: the fresh Cache is built before the lock is taken, so the
/// DB query never blocks the hot path, and a failed rebuild leaves the live
/// cache in place. Called at boot (serve.zig) and after every admin model-caps
/// mutation.
///
/// The cache is a PROCESS SINGLETON, so it owns its memory off
/// `std.heap.page_allocator` — not a caller-supplied allocator. An earlier
/// design threaded the caller's allocator through here; the admin CRUD handler
/// then passed its request-scoped `ctx.alloc`, leaving the global cache holding
/// request-lifetime memory (a use-after-free once the request arena reset, and a
/// cross-allocator free on the next build-then-swap). Owning the backing here
/// removes that footgun: no caller can tie cache lifetime to a transient scope.
pub fn populate(conn: *pg.Conn) !void {
    const fresh = try Cache.initFromConn(std.heap.page_allocator, conn);
    global_lock.lock();
    defer global_lock.unlock();
    if (global) |*g| g.deinit();
    global = fresh;
}

pub fn lookup_model_rate(provider: []const u8, model: []const u8) ?ModelRate {
    global_lock.lock();
    defer global_lock.unlock();
    if (global) |*g| return g.lookup(provider, model);
    return null;
}

pub fn deinit() void {
    global_lock.lock();
    defer global_lock.unlock();
    if (global) |*g| g.deinit();
    global = null;
}

// ── Tests (pure — no DB) ────────────────────────────────────────────────────

test "writeKey: the same model under two providers yields distinct keys" {
    // The cross-provider collision guard: claude-opus-4-8 on anthropic must NOT
    // map to the same rate-cache key as on pioneer (different rates).
    var a: [512]u8 = undefined;
    var b: [512]u8 = undefined;
    const ka = writeKey(&a, "anthropic", "claude-opus-4-8").?;
    const kb = writeKey(&b, "pioneer", "claude-opus-4-8").?;
    try std.testing.expect(!std.mem.eql(u8, ka, kb));
    // And the separator is the unit-separator, never a byte a provider/model carries.
    try std.testing.expectEqual(@as(usize, "anthropic".len), std.mem.indexOfScalar(u8, ka, KEY_SEP).?);
}

test "writeKey: returns null (cache miss, never wrong rate) when the pair overflows" {
    var small: [4]u8 = undefined;
    try std.testing.expect(writeKey(&small, "anthropic", "claude-opus-4-8") == null);
}
