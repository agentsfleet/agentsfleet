//! Per-model token rates, cached in front of `core.model_library`.
//!
//! Read by `tenant_billing` when it prices a slice, by provider activation when
//! it validates that a model is catalogued, and by the tenant registry page when
//! it displays rates. The catalogue is the authority; this is a cache in front of
//! it, on the shared `common.CacheTable` primitive.
//!
//! ## A miss is a question for the database, not an answer
//!
//! This module used to hold a COMPLETE snapshot of the catalogue in a growable
//! hash map, rebuilt wholesale at boot and after every admin mutation. Under that
//! shape a miss meant "no such model", and callers acted on it: the stage-charge
//! estimate panicked, renewal silently dropped to run-fee-only, and activation
//! rejected the model. That is only sound while the map is guaranteed complete.
//!
//! `common.CacheTable` is fixed-capacity and evicts, so completeness is not a
//! property it can offer — its own contract is "for values that can always be
//! recomputed; a miss is never an error". Adopting it therefore means adopting
//! that contract: every correctness consumer passes a connection, and a miss
//! loads the one row it asked about. What is given up is a synchronous
//! no-connection lookup on the charge path; what is bought is that an evicted or
//! never-loaded rate can no longer read as "this model is not in the catalogue",
//! which was a panic on one path and an unbilled slice on another.
//!
//! ## Freshness is the generation, not a deadline
//!
//! Entries never expire on a clock. The catalogue cannot change without
//! `core.model_catalogue_revision` advancing, so the generation an entry was read
//! at is stored WITH it and compared on every billing read: a caller that has
//! observed revision N accepts a cached entry only at N or later, and otherwise
//! reloads. A time-based deadline would add database reads on the charge path
//! without making a single stale answer impossible — the revision already does
//! that, and does it exactly.
//!
//! Two consumers, two guarantees, one table:
//!
//!   - `rateAtRevision` — billing and activation. Never returns a rate older
//!     than the generation the caller observed. Fails closed.
//!   - `cachedRate` — the registry page's display fields, which are already
//!     nullable. Takes whatever is resident and never issues a statement, because
//!     that page has a measured statement budget (§3) that a per-row load would
//!     blow.
//!
//! ## Identity
//!
//! The key is the `(provider, model)` pair held as two separate fields, compared
//! byte-for-byte. It is NOT a digest and NOT a delimiter-joined string. It used
//! to be `provider ++ 0x1f ++ model`, chosen because that byte "never appears in
//! a provider name or model_id" — an assumption about catalogue data that nothing
//! validated. A `0x1f` in either field made two DIFFERENT tuples produce the SAME
//! key:
//!
//!     ("a",      "b\x1fc")  ->  "a\x1fb\x1fc"
//!     ("a\x1fb", "c")       ->  "a\x1fb\x1fc"
//!
//! Both then selected whichever rate loaded last. Billing a request at another
//! model's price is the worst failure this module has, and it was one unusual
//! catalogue row away. Keeping the fields apart makes the collision
//! unrepresentable rather than unlikely: there is no byte string for two tuples
//! to agree on. A hashed fixed-size key would have reintroduced exactly that —
//! astronomically unlikely is not the same guarantee as impossible, and this is
//! the one module where the difference is money.

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

/// The `(provider, model)` identity. Both slices are owned by the cache and
/// freed when the entry departs — see `RateKeyContext.evicted`.
///
/// Public with its policy because collision-freedom is this module's central
/// guarantee, not an implementation detail: `model_rate_cache_key_test.zig`
/// asserts it directly against `hash`/`eql` rather than inferring it from
/// lookup behaviour, which would only ever sample the pairs a test thought to try.
pub const RateKey = struct {
    provider: []const u8,
    model: []const u8,
};

/// A rate and the catalogue generation it was read at. The generation rides with
/// the value rather than in the key so that one entry per model serves both
/// consumers: billing compares it, the display path ignores it. Keying by
/// generation instead would hold a separate entry per model per generation, and
/// every bump would leave a full catalogue of unreachable entries behind.
const CachedRate = struct {
    revision: i64,
    rate: ModelRate,
};

/// 1024 slots against a curated catalogue of ~100 rows. Capacity is a
/// performance knob here, not a correctness one — over-subscribing costs a
/// reload, never a wrong answer — so it is sized for headroom against an admin
/// growing the catalogue rather than tuned.
const BUCKET_COUNT: usize = 256;
const BUCKET_SIZE: u8 = 4;

/// Entries carry no deadline (see the module note), so the `now_ms` the table
/// takes for expiry comparisons is never consulted. Named rather than a bare `0`
/// so a reader does not go looking for the clock it came from.
const NO_DEADLINE: i64 = 0;

pub const RateKeyContext = struct {
    /// Lengths are folded in before their bytes, so ("ab","c") and ("a","bc")
    /// hash differently even though their concatenations match. Without the
    /// length prefix, `eql` would still be correct but every such pair would
    /// land in one bucket — and a bucket holds four entries, so a systematic
    /// collision is an eviction source rather than a harmless slowdown.
    pub fn hash(_: *const RateKeyContext, k: RateKey) u64 {
        var h = std.hash.Wyhash.init(0);
        h.update(std.mem.asBytes(&@as(u64, k.provider.len)));
        h.update(k.provider);
        h.update(std.mem.asBytes(&@as(u64, k.model.len)));
        h.update(k.model);
        return h.final();
    }

    pub fn eql(_: *const RateKeyContext, a: RateKey, b: RateKey) bool {
        return std.mem.eql(u8, a.provider, b.provider) and std.mem.eql(u8, a.model, b.model);
    }

    /// The table's single departure hook — eviction, same-key refresh, removal
    /// and `clear` all arrive here. The KEY owns memory in this cache (the value
    /// is plain data), so this is where the identity strings are freed, and the
    /// only place.
    ///
    /// It reads the module allocator rather than holding one, so that
    /// `setBackingAllocatorForTest` cannot leave resident entries to be freed by
    /// an allocator that did not allocate them. That swap clears the table first
    /// for the same reason.
    pub fn evicted(_: *const RateKeyContext, key: RateKey, _: CachedRate) void {
        backing_allocator.free(key.provider);
        backing_allocator.free(key.model);
    }
};

const Table = common.CacheTable(RateKey, CachedRate, RateKeyContext, .{
    .bucket_count = BUCKET_COUNT,
    .bucket_size = BUCKET_SIZE,
});

// ── Process-global cache ────────────────────────────────────────────────────

var table: Table = Table.init(.{});
var lock: common.RwLock = .{};

/// Backing allocator for the key strings. Process-lifetime `page_allocator` in
/// production; a leak test swaps in `testing.allocator` to audit the load/evict
/// cycle. A MODULE knob rather than a parameter: an earlier design threaded the
/// caller's allocator through, and the admin CRUD handler passed its
/// request-scoped `ctx.alloc`, leaving a process-lifetime cache holding
/// request-lifetime memory.
var backing_allocator: std.mem.Allocator = std.heap.page_allocator;

/// Test-only: swap the allocator backing cached keys. Clears first, so no
/// resident entry outlives the allocator that produced it. Returns the previous
/// allocator; the caller restores it when the test ends.
pub fn setBackingAllocatorForTest(alloc: std.mem.Allocator) std.mem.Allocator {
    clear();
    const prev = backing_allocator;
    backing_allocator = alloc;
    return prev;
}

/// The rate for `(provider, model)` as of `revision` or later.
///
/// `null` means the catalogue has no such row — an answer, from the database.
/// An error means the rate could not be established at all, which is NOT the
/// same answer and must never be treated as one: the caller fails closed rather
/// than falling back to whatever happens to be cached.
///
/// A cached entry is accepted only when its generation is at least the one the
/// caller observed. Accepting a LATER generation is deliberate — revisions only
/// increase, so a later one is fresher, and it is never the stale direction this
/// guards against. Reloading to match an older observation exactly is impossible
/// anyway: the database holds current state, not history.
pub fn rateAtRevision(
    conn: *pg.Conn,
    revision: i64,
    provider: []const u8,
    model: []const u8,
) !?ModelRate {
    if (peek(provider, model)) |cached| {
        if (cached.revision >= revision) return cached.rate;
    }
    const loaded = try loadRate(conn, provider, model);
    const rate = loaded.rate orelse return null;
    store(loaded.revision, provider, model, rate);
    return rate;
}

/// Whatever rate is resident, at any generation, without touching the database.
///
/// For display only. The tenant registry page renders every rate field as
/// nullable already, so a miss costs a blank cell — and that page's statement
/// budget is measured and pinned (§3), which a per-row load would breach.
pub fn cachedRate(provider: []const u8, model: []const u8) ?ModelRate {
    const cached = peek(provider, model) orelse return null;
    return cached.rate;
}

/// Smallest context window any provider publishes for `model`, or null when the
/// catalogue does not carry it.
///
/// A context window is a property of the MODEL; rates are a property of the
/// (host, model) pair. So a custom OpenAI-compatible endpoint can borrow the cap
/// without borrowing a price it is not billed at — which is why this returns only
/// the cap and never a `ModelRate`.
///
/// Answered by the database, not by scanning the cache. Hosts genuinely disagree
/// on the window for one model, so the answer is the MINIMUM across every
/// catalogue row for it — and a minimum over a bounded cache is a minimum over
/// whichever rows survived eviction. That error is one-directional and unsafe: a
/// budget above the real window fails the request mid-run at the provider.
pub fn contextCapForModel(conn: *pg.Conn, model: []const u8) !?u32 {
    var q = PgQuery.from(try conn.query(sql.MIN_CONTEXT_CAP_FOR_MODEL, .{model}));
    defer q.deinit();
    const row = (try q.next()) orelse return null;
    const cap = (try row.get(?i32, 0)) orelse return null;
    return @intCast(@max(cap, 0));
}

/// Drop every entry, freeing the key strings.
///
/// Called after an admin catalogue mutation on THIS replica. It is prompt
/// reclamation, never the correctness mechanism: a sibling replica clears
/// nothing and stays correct because its entries carry the old generation and
/// every billing read compares it.
pub fn clear() void {
    lock.lock();
    defer lock.unlock();
    table.clear();
}

pub fn deinit() void {
    clear();
}

/// Live entries — tests and gauges only.
pub fn count() usize {
    lock.lockShared();
    defer lock.unlockShared();
    return table.count(NO_DEADLINE);
}

// ── internals ───────────────────────────────────────────────────────────────

/// Non-mutating read under a shared lock, so concurrent charge computations do
/// not serialize behind one another. `peek` declines to refresh LRU position,
/// which is the price of not needing exclusive access; the catalogue working set
/// is far smaller than the table, so nothing contends for slots anyway.
fn peek(provider: []const u8, model: []const u8) ?CachedRate {
    lock.lockShared();
    defer lock.unlockShared();
    return table.peek(.{ .provider = provider, .model = model }, NO_DEADLINE);
}

/// Admit a rate. Best-effort by construction: an allocation failure skips the
/// insert and the caller still returns the rate it loaded. A cache that fails to
/// admit costs a reload, and the primitive's contract is that a miss is never an
/// error — so there is nothing here worth failing a charge over.
fn store(revision: i64, provider: []const u8, model: []const u8, rate: ModelRate) void {
    const p = backing_allocator.dupe(u8, provider) catch return;
    const m = backing_allocator.dupe(u8, model) catch {
        backing_allocator.free(p);
        return;
    };
    lock.lock();
    defer lock.unlock();
    table.put(
        .{ .provider = p, .model = m },
        .{ .revision = revision, .rate = rate },
        common.NEVER_EXPIRES,
        NO_DEADLINE,
    );
}

const LoadedRate = struct {
    /// The generation the row was read at. Present even when `rate` is null —
    /// that is the reason the statement drives its join from the singleton.
    revision: i64,
    rate: ?ModelRate,
};

/// One statement: the generation and the row it describes, from one snapshot.
fn loadRate(conn: *pg.Conn, provider: []const u8, model: []const u8) !LoadedRate {
    var q = PgQuery.from(try conn.query(sql.LOAD_RATE_WITH_REVISION, .{ provider, model }));
    defer q.deinit();
    const row = (try q.next()) orelse return error.CatalogueRevisionMissing;
    const revision = try row.get(i64, 0);
    const cap = (try row.get(?i32, 1)) orelse return .{ .revision = revision, .rate = null };
    return .{
        .revision = revision,
        .rate = .{
            .context_cap_tokens = @intCast(@max(cap, 0)),
            .input_nanos_per_mtok = (try row.get(?i64, 2)) orelse 0,
            .cached_input_nanos_per_mtok = (try row.get(?i64, 3)) orelse 0,
            .output_nanos_per_mtok = (try row.get(?i64, 4)) orelse 0,
        },
    };
}

test {
    _ = @import("model_rate_cache_key_test.zig");
}
