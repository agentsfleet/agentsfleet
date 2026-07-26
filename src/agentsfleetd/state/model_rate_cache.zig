//! Process-singleton cache of per-model token rates.
//!
//! Populated at API server boot from core.model_library; read on the hot path by
//! tenant_billing.computeStageCharge under platform-managed posture. The admin
//! model-library CRUD API calls populate() again after every mutation so a rate
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
const sql = @import("model_library/sql.zig");

pub const ModelRate = struct {
    input_nanos_per_mtok: i64,
    cached_input_nanos_per_mtok: i64,
    output_nanos_per_mtok: i64,
    context_cap_tokens: u32,
};

/// Rate-cache identity: the (provider, model) pair itself, held as two fields.
///
/// It used to be `provider ++ 0x1f ++ model` in a `StringHashMap`. The
/// separator was chosen because "it never appears in a provider name or
/// model_id" — but that is an assumption about catalogue data, not a property
/// of the encoding, and nothing validates it. A provider or model carrying a
/// `0x1f` byte makes two DIFFERENT tuples produce the SAME key:
///
///     ("a",     "b\x1fc")  ->  "a\x1fb\x1fc"
///     ("a\x1fb", "c")      ->  "a\x1fb\x1fc"
///
/// Both then select whichever rate was loaded last, and `contextCapForModel`
/// split on the FIRST separator, so it read one of the two as a model named
/// `b\x1fc`. Billing a request at another model's price is the worst failure
/// this module has, and it was one unusual catalogue row away.
///
/// Keeping the fields apart makes the collision unrepresentable rather than
/// unlikely: there is no byte string for two tuples to agree on. It also drops
/// the old 512-byte key buffer, which silently skipped any pair that overflowed
/// it — at load time AND at lookup, so a long pair was a permanent miss.
pub const RateKey = struct {
    provider: []const u8,
    model: []const u8,
};

const RateKeyContext = struct {
    /// Lengths are folded in before their bytes, so ("ab","c") and ("a","bc")
    /// hash differently even though their concatenations match. Without the
    /// length prefix the hash reintroduces exactly the ambiguity the struct
    /// removes — `eql` would still be correct, but every such pair would
    /// collide into one bucket.
    pub fn hash(_: RateKeyContext, k: RateKey) u64 {
        var h = std.hash.Wyhash.init(0);
        h.update(std.mem.asBytes(&@as(u64, k.provider.len)));
        h.update(k.provider);
        h.update(std.mem.asBytes(&@as(u64, k.model.len)));
        h.update(k.model);
        return h.final();
    }

    pub fn eql(_: RateKeyContext, a: RateKey, b: RateKey) bool {
        return std.mem.eql(u8, a.provider, b.provider) and std.mem.eql(u8, a.model, b.model);
    }
};

const RatesMap = std.HashMapUnmanaged(RateKey, ModelRate, RateKeyContext, std.hash_map.default_max_load_percentage);

pub const Cache = struct {
    const Self = @This();

    arena: std.heap.ArenaAllocator,
    rates: RatesMap,

    pub fn initFromConn(alloc: std.mem.Allocator, conn: *pg.Conn) !Cache {
        var arena = std.heap.ArenaAllocator.init(alloc);
        errdefer arena.deinit();
        const arena_alloc = arena.allocator();

        var rates: RatesMap = .{};
        var q = PgQuery.from(try conn.query(sql.LIST_RATES_FOR_CACHE, .{}));
        defer q.deinit();
        while (try q.next()) |row| {
            const provider = try row.get([]const u8, 0);
            const model_id = try row.get([]const u8, 1);
            const cap_i32 = try row.get(i32, 2);
            const in_rate = try row.get(i64, 3);
            const cached_rate = try row.get(i64, 4);
            const out_rate = try row.get(i64, 5);
            const key = RateKey{
                .provider = try arena_alloc.dupe(u8, provider),
                .model = try arena_alloc.dupe(u8, model_id),
            };
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

    /// Test-only: an empty cache owning its own arena, so the key-identity tests
    /// can assert on lookup behaviour without a database. The alternative —
    /// seeding a catalogue and rebuilding — would put a live connection between
    /// the test and the property it is checking, which is pure key semantics.
    pub fn emptyForTest(alloc: std.mem.Allocator) Cache {
        return .{ .arena = std.heap.ArenaAllocator.init(alloc), .rates = .{} };
    }

    /// Test-only: insert one identity, duplicating the key bytes into the arena
    /// exactly as `initFromConn` does — so the tests exercise the same ownership
    /// the production path produces, not borrowed literals.
    pub fn putForTest(self: *Self, provider: []const u8, model: []const u8, r: ModelRate) !void {
        const a = self.arena.allocator();
        try self.rates.put(a, .{
            .provider = try a.dupe(u8, provider),
            .model = try a.dupe(u8, model),
        }, r);
    }

    pub fn lookup(self: *const Self, provider: []const u8, model: []const u8) ?ModelRate {
        return self.rates.get(.{ .provider = provider, .model = model });
    }

    /// Context cap for a model under ANY provider.
    ///
    /// A context window is a property of the MODEL; rates are a property of the
    /// (host, model) pair. kimi-k3 is 1,048,576 tokens whether it is reached via
    /// Moonshot, Novita, or a custom OpenAI-compatible endpoint pointed at either.
    /// So a custom endpoint can borrow the cap without borrowing a price it is
    /// not billed at — which is why this deliberately returns only the cap and
    /// never a ModelRate.
    ///
    /// Linear scan: the catalogue is a curated ~100 rows and this runs on entry
    /// activation, not the hot path. Hosts genuinely disagree on the window for
    /// the same model (GLM-5.2 is 262k on Together and 1M on Pioneer), so this
    /// takes the MINIMUM across matches — deterministic regardless of hash
    /// iteration order, and conservative: a budget under the real window wastes
    /// headroom, a budget over it fails the request mid-run at the provider.
    pub fn contextCapForModel(self: *const Self, model: []const u8) ?u32 {
        var min_cap: ?u32 = null;
        var it = self.rates.iterator();
        while (it.next()) |entry| {
            // Compares the model FIELD. The previous form split the composite
            // key on its first separator, so a provider containing that byte
            // made this read part of the provider as the model name.
            if (!std.mem.eql(u8, entry.key_ptr.model, model)) continue;
            const cap = entry.value_ptr.context_cap_tokens;
            min_cap = if (min_cap) |m| @min(m, cap) else cap;
        }
        return min_cap;
    }
};

// ── Process-global singleton (initialized at API boot) ─────────────────────

var global: ?Cache = null;
var global_lock: common.Mutex = .{};

/// Backing allocator for the process singleton's arena. Defaults to the
/// process-lifetime `page_allocator`; a leak test overrides it to
/// `testing.allocator` (via `setBackingAllocatorForTest`) to audit the
/// rebuild/swap cycle, then restores it. It is a MODULE knob, never a `populate`
/// parameter — see `populate`'s doc comment for why no caller may supply one.
var backing_allocator: std.mem.Allocator = std.heap.page_allocator;

/// Test-only: swap the singleton's backing allocator so a leak test can audit
/// the populate/swap cycle under `testing.allocator`. Returns the previous
/// allocator; the caller restores it when the test ends.
pub fn setBackingAllocatorForTest(alloc: std.mem.Allocator) std.mem.Allocator {
    const prev = backing_allocator;
    backing_allocator = alloc;
    return prev;
}

/// (Re)build the rate cache from core.model_library. Safe to call at runtime under
/// concurrent readers: the fresh Cache is built before the lock is taken, so the
/// DB query never blocks the hot path, and a failed rebuild leaves the live
/// cache in place. Called at boot (serve.zig) and after every admin
/// model-library mutation.
///
/// The cache is a PROCESS SINGLETON, so it owns its memory off the module
/// `backing_allocator` (`page_allocator` in production) — never a caller-supplied
/// allocator. An earlier design threaded the caller's allocator through here; the
/// admin CRUD handler then passed its request-scoped `ctx.alloc`, leaving the
/// global cache holding request-lifetime memory (a use-after-free once the
/// request arena reset, and a cross-allocator free on the next build-then-swap).
/// Owning the backing at module scope removes that footgun: no caller can tie
/// cache lifetime to a transient scope, and a leak test overrides it in place.
pub fn populate(conn: *pg.Conn) !void {
    const fresh = try Cache.initFromConn(backing_allocator, conn);
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

/// Cap-only lookup across providers — see Cache.contextCapForModel.
pub fn lookup_context_cap(model: []const u8) ?u32 {
    global_lock.lock();
    defer global_lock.unlock();
    if (global) |*g| return g.contextCapForModel(model);
    return null;
}

pub fn deinit() void {
    global_lock.lock();
    defer global_lock.unlock();
    if (global) |*g| g.deinit();
    global = null;
}

// ── Tests (pure — no DB) ────────────────────────────────────────────────────

test "RateKey: the same model under two providers stays distinct" {
    // The cross-provider guard: claude-opus-4-8 on anthropic must not select
    // the pioneer rate, which is a different price for the same model name.
    const ctx = RateKeyContext{};
    const a = RateKey{ .provider = "anthropic", .model = "claude-opus-4-8" };
    const b = RateKey{ .provider = "pioneer", .model = "claude-opus-4-8" };
    try std.testing.expect(!ctx.eql(a, b));
    try std.testing.expect(ctx.hash(a) != ctx.hash(b));
}

test "RateKey: a pair too long for the old 512-byte buffer still resolves" {
    // The previous key encoding skipped any pair that overflowed its buffer —
    // at load AND at lookup — so a long provider/model was a permanent miss
    // that billing saw as "no rate" rather than as an error.
    const alloc = std.testing.allocator;
    const long_provider = try alloc.alloc(u8, 600);
    defer alloc.free(long_provider);
    @memset(long_provider, 'p');

    var rates: RatesMap = .{};
    defer rates.deinit(alloc);
    try rates.put(alloc, .{ .provider = long_provider, .model = "m" }, .{
        .input_nanos_per_mtok = 7,
        .cached_input_nanos_per_mtok = 0,
        .output_nanos_per_mtok = 0,
        .context_cap_tokens = 1,
    });

    const got = rates.get(.{ .provider = long_provider, .model = "m" });
    try std.testing.expect(got != null);
    try std.testing.expectEqual(@as(i64, 7), got.?.input_nanos_per_mtok);
}
