//! Answers "what may this person do, right now?" for the durable command-line
//! credential, by asking the identity provider and caching the answer briefly.
//!
//! This is the one credential source whose capabilities are not authored in
//! `scopes.zig`. The credential proves identity; the provider owns capability.
//! `docs/AUTH.md` records why — a fixed grant would widen a narrowly-
//! provisioned collaborator the moment they ran `login`, and snapshotting the
//! claim onto the credential row would make this a second store of a fact the
//! provider owns, frozen at issue time.
//!
//! The cache is a latency optimisation and nothing else. It holds no
//! authority: it is in memory, it never outlives the process, and every entry
//! self-heals toward the provider within the freshness window — so there is
//! no projection to backfill, order, or reconcile.
//!
//! Behaviour under a provider outage mirrors what the token path already does
//! when a key-set fetch fails. A warm entry is served up to a hard ceiling far
//! above the freshness window, because refusing every terminal during a vendor
//! blip is worse than acting on capabilities that are minutes old. Past the
//! ceiling, or with nothing cached, the caller is told authentication is
//! unavailable rather than handed an empty set that would read as "you were
//! demoted".
//!
//! Not single-flighted, deliberately. The token path carries single-flight
//! machinery because a key-set miss can stampede every request at once; here
//! the fan-out is per person, the window is a minute, and concurrency is
//! already bounded by the server's worker count. The machinery would cost more
//! than it saves, and it can be added behind this same surface if a deployment
//! ever proves otherwise.

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const logging = @import("log");
const ec = @import("auth_codes");
const clerk_scope_fetch = @import("clerk_scope_fetch.zig");

const log = logging.scoped(.clerk_scopes);

const MS_PER_SECOND = 1000;
const MS_PER_MINUTE = 60 * MS_PER_SECOND;

/// How long a fetched claim is served without asking again. A revocation in
/// the provider's dashboard reaches a terminal within this window, which is
/// the same order as the dashboard's own session-token refresh — the parity
/// the resolved model exists to keep.
pub const DEFAULT_TTL_MS: i64 = 60 * MS_PER_SECOND;

/// Hard age limit on serving a cached claim when the provider cannot be
/// reached. Past this an outage stops being something to ride out: the answer
/// is old enough that acting on it could contradict a revocation nobody can
/// confirm, so the request is refused instead.
pub const DEFAULT_STALE_CEILING_MS: i64 = 15 * MS_PER_MINUTE;

/// Distinct subjects held at once. Sized well above any deployment's
/// concurrently-active operator count; reaching it means the process has seen
/// far more people than it is serving, so the whole map is dropped and rebuilt
/// rather than left to grow. Dropping costs one cold round of fetches and
/// restores full caching immediately — refusing new entries instead would
/// leave every newly-seen person permanently uncached.
pub const MAX_CACHED_SUBJECTS: usize = 4096;

const EV_SERVED_STALE = "scopes_served_stale";
const EV_UNAVAILABLE = "scopes_unavailable";
const EV_SUBJECT_UNKNOWN = "scopes_subject_unknown_to_provider";
const EV_CACHE_RESET = "scopes_cache_reset_at_bound";

pub const ResolveError = error{ ScopesUnavailable, OutOfMemory };

/// One cached answer. `claim` is owned by the resolver's allocator.
const Entry = struct {
    claim: []const u8,
    fetched_at_ms: i64,
};

pub const Config = struct {
    /// Borrowed from the boot-resolved provider secret. Absent means the
    /// deployment cannot resolve capabilities at all, which is an outage
    /// rather than an empty grant — every resolve refuses.
    secret: ?[]const u8,
    ttl_ms: i64 = DEFAULT_TTL_MS,
    stale_ceiling_ms: i64 = DEFAULT_STALE_CEILING_MS,
};

/// Stored allocator: this type owns every cached key and claim for the life of
/// the process, so `deinit` frees without a second allocator to get wrong.
pub const ScopeResolver = struct {
    const Self = @This();

    alloc: std.mem.Allocator,
    secret: ?[]const u8,
    ttl_ms: i64,
    stale_ceiling_ms: i64,
    mutex: common.Mutex = .{},
    cache: std.StringHashMap(Entry),

    pub fn init(alloc: std.mem.Allocator, cfg: Config) Self {
        return .{
            .alloc = alloc,
            .secret = cfg.secret,
            .ttl_ms = cfg.ttl_ms,
            .stale_ceiling_ms = cfg.stale_ceiling_ms,
            .cache = std.StringHashMap(Entry).init(alloc),
        };
    }

    pub fn deinit(self: *Self) void {
        self.mutex.lock();
        defer self.mutex.unlock();
        self.dropAllLocked();
        self.cache.deinit();
    }

    /// Resolve the capability claim for `oidc_subject`. The caller owns the
    /// returned bytes and frees them with `alloc` — the SAME allocator passed
    /// in, never the resolver's.
    ///
    /// The provider round trip happens with the lock RELEASED, so a request
    /// answered from cache never queues behind a slow provider.
    pub fn resolve(
        self: *Self,
        alloc: std.mem.Allocator,
        oidc_subject: []const u8,
    ) ResolveError![]const u8 {
        var stale: ?[]const u8 = null;
        {
            self.mutex.lock();
            defer self.mutex.unlock();
            if (self.cache.get(oidc_subject)) |entry| {
                const age_ms = clock.nowMillis() - entry.fetched_at_ms;
                if (age_ms <= self.ttl_ms) {
                    return alloc.dupe(u8, entry.claim) catch ResolveError.OutOfMemory;
                }
                // Copied under the lock so nothing borrows a map value that a
                // concurrent store could free while the fetch is in flight.
                if (age_ms <= self.stale_ceiling_ms) stale = alloc.dupe(u8, entry.claim) catch null;
            }
        }

        const fresh = clerk_scope_fetch.fetchScopeClaim(alloc, self.secret, oidc_subject) catch |err| {
            return self.onFetchFailed(alloc, err, stale);
        };
        if (stale) |s| alloc.free(s);
        self.store(oidc_subject, fresh);
        return fresh;
    }

    /// A subject the provider has never heard of resolves to no capabilities
    /// rather than to an outage: the person behind the credential is gone, and
    /// every gate should refuse them by name instead of telling their terminal
    /// to try again later. Deliberately not cached — a deletion is permanent
    /// and needs no cache, while anything transient must not blank a live
    /// operator for a full freshness window.
    fn onFetchFailed(
        self: *Self,
        alloc: std.mem.Allocator,
        err: clerk_scope_fetch.FetchError,
        stale: ?[]const u8,
    ) ResolveError![]const u8 {
        if (err == error.NotFound) {
            if (stale) |s| alloc.free(s);
            log.warn(EV_SUBJECT_UNKNOWN, .{ .error_code = ec.ERR_AUTH_UNAVAILABLE });
            return alloc.dupe(u8, clerk_scope_fetch.UNPROVISIONED_CLAIM) catch
                ResolveError.OutOfMemory;
        }
        if (stale) |s| {
            log.warn(EV_SERVED_STALE, .{
                .error_code = ec.ERR_AUTH_UNAVAILABLE,
                .err = @errorName(err),
                .ceiling_ms = self.stale_ceiling_ms,
            });
            return s;
        }
        log.err(EV_UNAVAILABLE, .{ .error_code = ec.ERR_AUTH_UNAVAILABLE, .err = @errorName(err) });
        return ResolveError.ScopesUnavailable;
    }

    /// Record a freshly-fetched claim. Best-effort by design: a cache write
    /// that cannot allocate costs the next request one round trip and nothing
    /// else, so it is never worth failing an authenticated request over.
    fn store(self: *Self, oidc_subject: []const u8, claim: []const u8) void {
        // Duplicated BEFORE the swap so a failure leaves the existing entry
        // intact rather than freeing it and leaving a dangling value.
        const owned_claim = self.alloc.dupe(u8, claim) catch return;
        const fetched_at_ms = clock.nowMillis();

        self.mutex.lock();
        defer self.mutex.unlock();

        if (self.cache.getEntry(oidc_subject)) |existing| {
            self.alloc.free(existing.value_ptr.claim);
            existing.value_ptr.* = .{ .claim = owned_claim, .fetched_at_ms = fetched_at_ms };
            return;
        }
        if (self.cache.count() >= MAX_CACHED_SUBJECTS) {
            log.warn(EV_CACHE_RESET, .{
                .error_code = ec.ERR_INTERNAL_OPERATION_FAILED,
                .bound = MAX_CACHED_SUBJECTS,
            });
            self.dropAllLocked();
        }
        const owned_key = self.alloc.dupe(u8, oidc_subject) catch {
            self.alloc.free(owned_claim);
            return;
        };
        self.cache.put(owned_key, .{ .claim = owned_claim, .fetched_at_ms = fetched_at_ms }) catch {
            self.alloc.free(owned_key);
            self.alloc.free(owned_claim);
        };
    }

    /// Free every key and claim and empty the map. Caller holds `mutex`.
    fn dropAllLocked(self: *Self) void {
        var it = self.cache.iterator();
        while (it.next()) |kv| {
            self.alloc.free(kv.key_ptr.*);
            self.alloc.free(kv.value_ptr.claim);
        }
        self.cache.clearRetainingCapacity();
    }
};

/// Type-erased adapter matching the middleware's `ScopeFn`. The middleware
/// holds this behind an opaque pointer so its branches stay provable without a
/// network, exactly as the credential lookup does.
pub fn resolveScopes(
    scope_host: *anyopaque,
    alloc: std.mem.Allocator,
    oidc_subject: []const u8,
) anyerror![]const u8 {
    const self: *ScopeResolver = @ptrCast(@alignCast(scope_host));
    return self.resolve(alloc, oidc_subject);
}

// In-file tests: the cache is private on purpose, and its policy — what is
// served fresh, what survives an outage, what is refused, and what is freed —
// is observable only from inside this module. Leaving the provider secret
// unset makes every fetch fail without a network, which is exactly the outage
// these branches exist for. Behaviour reachable through the public surface is
// tested in the sibling file.

const testing = std.testing;
const SUBJECT = "user_2aXyTest";
const CLAIM = "fleet:read model:read";
/// Ages every entry instantly: a real age is never negative, so `age <= -1`
/// is false for both windows. Deterministic where a sleep would be flaky.
const ALWAYS_EXPIRED_MS: i64 = -1;

fn testResolver(ttl_ms: i64, stale_ceiling_ms: i64) ScopeResolver {
    return ScopeResolver.init(testing.allocator, .{
        .secret = null,
        .ttl_ms = ttl_ms,
        .stale_ceiling_ms = stale_ceiling_ms,
    });
}

test "a fresh entry is served without asking the provider" {
    var resolver = testResolver(DEFAULT_TTL_MS, DEFAULT_STALE_CEILING_MS);
    defer resolver.deinit();
    resolver.store(SUBJECT, CLAIM);

    // No secret is configured, so any fetch would fail. Getting the claim back
    // proves the freshness window answered on its own.
    const claim = try resolver.resolve(testing.allocator, SUBJECT);
    defer testing.allocator.free(claim);
    try testing.expectEqualStrings(CLAIM, claim);
}

test "a stale entry within the ceiling survives a provider outage" {
    var resolver = testResolver(ALWAYS_EXPIRED_MS, DEFAULT_STALE_CEILING_MS);
    defer resolver.deinit();
    resolver.store(SUBJECT, CLAIM);

    // Refusing every terminal during a vendor blip is worse than acting on
    // capabilities that are minutes old.
    const claim = try resolver.resolve(testing.allocator, SUBJECT);
    defer testing.allocator.free(claim);
    try testing.expectEqualStrings(CLAIM, claim);
}

test "past the ceiling the caller is refused, never handed an empty set" {
    var resolver = testResolver(ALWAYS_EXPIRED_MS, ALWAYS_EXPIRED_MS);
    defer resolver.deinit();
    resolver.store(SUBJECT, CLAIM);

    // An empty set here would read to the operator as a demotion they never
    // received; an outage says what actually happened.
    try testing.expectError(
        ResolveError.ScopesUnavailable,
        resolver.resolve(testing.allocator, SUBJECT),
    );
}

test "a cold subject with no reachable provider is an outage, not an empty grant" {
    var resolver = testResolver(DEFAULT_TTL_MS, DEFAULT_STALE_CEILING_MS);
    defer resolver.deinit();
    try testing.expectError(
        ResolveError.ScopesUnavailable,
        resolver.resolve(testing.allocator, SUBJECT),
    );
}

test "a subject the provider does not know resolves to no capabilities" {
    var resolver = testResolver(DEFAULT_TTL_MS, DEFAULT_STALE_CEILING_MS);
    defer resolver.deinit();

    const claim = try resolver.onFetchFailed(testing.allocator, error.NotFound, null);
    defer testing.allocator.free(claim);
    try testing.expectEqualStrings(clerk_scope_fetch.UNPROVISIONED_CLAIM, claim);
    // Never cached: a deletion is permanent and needs no cache, and anything
    // transient must not blank a live operator for a whole freshness window.
    try testing.expectEqual(@as(usize, 0), resolver.cache.count());
}

test "re-storing a subject replaces its claim without leaking the previous one" {
    var resolver = testResolver(DEFAULT_TTL_MS, DEFAULT_STALE_CEILING_MS);
    defer resolver.deinit();
    resolver.store(SUBJECT, CLAIM);
    resolver.store(SUBJECT, "fleet:admin");

    try testing.expectEqual(@as(usize, 1), resolver.cache.count());
    const claim = try resolver.resolve(testing.allocator, SUBJECT);
    defer testing.allocator.free(claim);
    try testing.expectEqualStrings("fleet:admin", claim);
}

test "the cache is dropped rather than grown past its bound" {
    var resolver = testResolver(DEFAULT_TTL_MS, DEFAULT_STALE_CEILING_MS);
    defer resolver.deinit();

    var i: usize = 0;
    while (i <= MAX_CACHED_SUBJECTS) : (i += 1) {
        var key_buf: [32]u8 = undefined;
        const key = try std.fmt.bufPrint(&key_buf, "user_{d}", .{i});
        resolver.store(key, CLAIM);
    }
    // The bound holds, and the leak detector proves the drop freed every key
    // and claim it discarded rather than orphaning them.
    try testing.expect(resolver.cache.count() <= MAX_CACHED_SUBJECTS);
    try testing.expect(resolver.cache.count() > 0);
}

test {
    // Keeps every declaration analysed even where nothing in this tree calls
    // it yet — an unreferenced body is never type-checked.
    std.testing.refAllDecls(@This());
    _ = @import("clerk_scope_resolver_test.zig");
}
