//! On-demand credential broker — one daemon singleton shared across the httpz
//! request threads and serving every sandbox's token mints. Resolves a vault
//! handle to a short-lived token via the config-driven integration registry
//! (`integration.zig`), caching the result. Adding a connector is a registry
//! descriptor, never a branch here (RULE CFG).
//!
//! Storage is `karlseguin/cache.zig` (vetted, not hand-rolled): sharded segments
//! (no global lock), `std.Io.RwLock` SHARED reads (concurrent readers, never an
//! exclusive lock on the hot path), atomic-refcounted entries (the token is used
//! lock-free after a few-ns shared-lock lookup), plus LRU + size bounds. The
//! broker owns only two things: the EXPIRY policy (injected `now_ms`, so tests are
//! deterministic and a token is re-minted a skew before its real expiry) and the
//! mint dispatch. The App key is loaded ONCE into `deps` (never per-mint).
//!
//!   resolve ─▶ store.get(key)  [shared RwLock, refcounted entry, lock-free use]
//!               │ hit & unexpired ─▶ dup token ─▶ ok
//!               │ miss / expired  ─▶ runMint (NO lock) ─▶ store.put(ttl) ─▶ dup ─▶ ok
//!
//! Cold-miss coordination: cache.zig's get/put does not single-flight, so the
//! thin per-key in-flight guard in `broker_flight.zig` sits on top — exactly
//! one caller mints per key; losers wait and re-read what the winner cached.
//! Without it, two simultaneous cold-misses on a ROTATING refresh provider
//! both post the same refresh token and the provider's reuse detection can
//! revoke the token family.

const CredentialBroker = @This();

/// Re-mint this many ms BEFORE the upstream expiry so a token handed to a tool
/// call has slack to complete (RULE UFS).
pub const EXPIRY_SKEW_MS: i64 = 60_000;

/// Cache shape: 64 segments (independent RwLocks) bounds cross-workspace
/// contention; the size cap bounds memory (LRU evicts the rest).
const CACHE_SEGMENTS: u16 = 64;
const CACHE_MAX_ENTRIES: u32 = 8192;

/// Metrics `outcome` labels (RULE UFS — shared by every emit site).
const OUTCOME_OK: []const u8 = "ok";
const OUTCOME_RECONNECT: []const u8 = "reconnect_required";
const OUTCOME_MINT_FAILED: []const u8 = "mint_failed";
const OUTCOME_UNKNOWN: []const u8 = "unknown_integration";

/// A cached token + its validity bound. cache.zig stores this by value and calls
/// `removedFromCache` on eviction to free the token bytes we own.
const TokenVal = struct {
    token: []const u8,
    expires_at_ms: i64,

    pub fn removedFromCache(self: *TokenVal, allocator: std.mem.Allocator) void {
        // @constCast is sound: the token is our own mutable dupe (cacheMinted).
        secure_memory.freeBytes(allocator, @constCast(self.token));
    }
};

const TokenCache = cache.Cache(TokenVal);

alloc: std.mem.Allocator,
registry: []const Spec,
deps: integration.Deps,
store: TokenCache,
/// Per-process Wyhash seed for the identity fingerprint. The fingerprint only
/// ever compares against itself within this broker's cache, so a random seed
/// costs nothing — and keeps handle-influenced collisions from being
/// precomputable offline.
fp_seed: u64,
/// Single-flight registry for cold-miss mints (`broker_flight.zig`): a key
/// present here is being minted right now; losers poll for its removal and
/// re-read the cache. The mutex guards `inflight` and nothing else.
inflight_mutex: common.Mutex = .{},
inflight: std.StringHashMapUnmanaged(void) = .empty,
/// Bounded loser wait (`broker_flight.zig` owns the default and rationale);
/// a test injects a short bound to prove the timeout without a 30 s park.
loser_wait_bound_ms: i64 = flight.LOSER_WAIT_BOUND_MS,
/// Injectable mint-latency clock; expiry math stays on the caller's `now_ms`.
latency_clock: *const fn () i64 = &common.clock.nowMillis,

/// `registry` is injected (production passes `integration.REGISTRY`) so a test can
/// supply a fake-id registry and prove dispatch is data-driven. `deps` carries the
/// daemon-singleton effects (the App key loaded ONCE, the HTTP boundary, the RS256
/// signer, the metrics hook) folded into every `MintCtx`.
pub fn init(alloc: std.mem.Allocator, registry: []const Spec, deps: integration.Deps) !CredentialBroker {
    var seed_bytes: [@sizeOf(u64)]u8 = undefined;
    try common.secureRandomBytes(&seed_bytes);
    return .{
        .alloc = alloc,
        .registry = registry,
        .deps = deps,
        .store = try TokenCache.init(common.globalIo(), alloc, .{
            .segment_count = CACHE_SEGMENTS,
            .max_size = CACHE_MAX_ENTRIES,
        }),
        .fp_seed = std.mem.readInt(u64, &seed_bytes, .little),
    };
}

pub fn deinit(self: *CredentialBroker) void {
    self.store.deinit();
    // Residual flight keys exist only if a minter died mid-flight; free them
    // so teardown is leak-clean either way.
    var it = self.inflight.keyIterator();
    while (it.next()) |k| self.alloc.free(k.*);
    self.inflight.deinit(self.alloc);
    self.* = undefined;
}

/// Resolve `integration_id` for `workspace` to a short-lived token, minting via the
/// registry on a cache miss. `now_ms` is injected (production passes the clock) for
/// deterministic expiry. The returned `ok.token` is duped with `alloc`
/// (caller-owned) — never an alias into the cache.
pub fn mint(
    self: *CredentialBroker,
    alloc: std.mem.Allocator,
    workspace: []const u8,
    integration_id: []const u8,
    handle: std.json.Value,
    now_ms: i64,
    binding: ?integration.RepositoryBinding,
) !integration.MintResult {
    const t0 = self.latency_clock();
    const id = parseIntegration(handle) orelse {
        self.emit(integration_id, OUTCOME_UNKNOWN, false, self.latency_clock() - t0);
        return .unknown_integration;
    };
    var key_buf: [512]u8 = undefined;
    // The binding is part of the cache IDENTITY, not just the mint input. Two
    // fleets in ONE workspace minting the same integration from the same
    // installation handle agree on workspace + id + handle fingerprint, so
    // without this a read-scoped fleet could be served the write-scoped token
    // its neighbour cached (and vice versa) — silently undoing the narrowing.
    const key = broker_key.writeKey(&key_buf, workspace, @tagName(id), broker_key.identityFingerprint(self.fp_seed, handle), broker_key.bindingFingerprint(binding)) orelse {
        self.emit(@tagName(id), OUTCOME_MINT_FAILED, false, self.latency_clock() - t0);
        return .{ .mint_failed = .permanent };
    };

    if (self.cachedToken(alloc, key, @tagName(id), now_ms, t0)) |res| return res;

    // Single-flight (broker_flight.zig, full rationale there): exactly one
    // cold-miss mint per key; losers re-read what the winner cached. An
    // unestablishable guard fails closed — an unguarded concurrent mint can
    // cost the whole token family.
    var claim = flight.beginFlight(self, key);
    while (claim == .lost) {
        if (self.cachedToken(alloc, key, @tagName(id), now_ms, t0)) |res| return res;
        claim = flight.beginFlight(self, key);
    }
    if (claim == .unavailable) {
        self.emit(@tagName(id), OUTCOME_MINT_FAILED, false, self.latency_clock() - t0);
        return .{ .mint_failed = .transient };
    }
    defer flight.endFlight(self, key);

    switch (self.runMint(id, handle, now_ms, binding)) {
        .ok => |minted| return self.finishColdMint(alloc, key, @tagName(id), minted, now_ms, t0),
        .reconnect_required => {
            self.emit(@tagName(id), OUTCOME_RECONNECT, false, self.latency_clock() - t0);
            return .reconnect_required;
        },
        .mint_failed => |retry| {
            self.emit(@tagName(id), OUTCOME_MINT_FAILED, false, self.latency_clock() - t0);
            return .{ .mint_failed = retry };
        },
    }
}

/// Cache + hand back a cold-path mint. The strategy's owned copies are freed
/// here exactly once; the caller receives independent dupes — including the
/// rotated refresh token when the exchange rotated it (RULE OWN: one free path
/// per allocation, proven leak-free under `std.testing.allocator`).
fn finishColdMint(self: *CredentialBroker, alloc: std.mem.Allocator, key: []const u8, id_name: []const u8, minted: integration.Minted, now_ms: i64, t0: i64) integration.MintResult {
    // runMint handed us owned copies; zeroize on release. The @constCast is
    // sound: every strategy dupes into fresh mutable memory.
    defer secure_memory.freeBytes(self.alloc, @constCast(minted.token));
    defer if (minted.rotated_refresh_token) |rt| secure_memory.freeBytes(self.alloc, @constCast(rt));
    const tok = alloc.dupe(u8, minted.token) catch {
        // A dupe OOM still emits: a silent failure would hide real mint churn.
        self.emit(id_name, OUTCOME_MINT_FAILED, false, self.latency_clock() - t0);
        return .{ .mint_failed = .transient };
    };
    // Degrade, don't fail, when only the ROTATED copy cannot be duped: the
    // exchange already consumed the old refresh token and the caller's access
    // token is in hand. Failing here would waste the mint AND have the retry
    // post the dead token immediately; dropping the rotation instead costs at
    // most the documented one-reconnect bound at expiry.
    const rotated: ?[]const u8 = if (minted.rotated_refresh_token) |rt|
        alloc.dupe(u8, rt) catch null
    else
        null;
    // Cache LAST: a mint that fails closed above must not leave a warm entry
    // (a hit reports no rotated token, so the caller would never re-persist).
    flight.cacheMinted(self, key, minted.token, minted.expires_at_ms, now_ms);
    self.emit(id_name, OUTCOME_OK, false, self.latency_clock() - t0);
    return .{ .ok = .{ .token = tok, .expires_at_ms = minted.expires_at_ms, .rotated_refresh_token = rotated } };
}

/// Fresh-enough cached token for `key`, duped into `alloc`. Null on a miss or
/// a skew-expired entry (the caller re-mints; the put overwrites).
fn cachedToken(self: *CredentialBroker, alloc: std.mem.Allocator, key: []const u8, id_name: []const u8, now_ms: i64, t0: i64) ?integration.MintResult {
    const entry = self.store.get(key) orelse return null;
    defer entry.release();
    if (now_ms >= entry.value.expires_at_ms - EXPIRY_SKEW_MS) return null;
    const tok = alloc.dupe(u8, entry.value.token) catch {
        self.emit(id_name, OUTCOME_MINT_FAILED, true, self.latency_clock() - t0);
        return .{ .mint_failed = .transient };
    };
    self.emit(id_name, OUTCOME_OK, true, self.latency_clock() - t0);
    // A hit did no exchange, so rotated_refresh_token stays null.
    return .{ .ok = .{ .token = tok, .expires_at_ms = entry.value.expires_at_ms } };
}

/// Dispatch to the integration's mint with a fully-built `MintCtx`. Runs WITHOUT
/// any cache lock held (the network call must not serialize other minters).
fn runMint(self: *CredentialBroker, id: integration.Id, handle: std.json.Value, now_ms: i64, binding: ?integration.RepositoryBinding) integration.Outcome {
    const spec = integration.resolve(self.registry, id) orelse return .{ .mint_failed = .permanent };
    const ctx = integration.MintCtx{
        .alloc = self.alloc,
        .handle = handle,
        .now_ms = now_ms,
        .platform = self.deps.platform,
        .http = self.deps.http,
        .sign = self.deps.sign,
        .repository_binding = binding,
    };
    // The strategy union owns dispatch; the broker never branches on id.
    return spec.mint.run(ctx) catch .{ .mint_failed = .transient };
}

fn emit(self: *CredentialBroker, integration_name: []const u8, outcome: []const u8, cache_hit: bool, latency_ms: i64) void {
    self.deps.metrics.onMint(.{
        .integration = integration_name,
        .outcome = outcome,
        .latency_ms = latency_ms,
        .cache_hit = cache_hit,
    });
}

fn parseIntegration(handle: std.json.Value) ?integration.Id {
    const obj = switch (handle) {
        .object => |o| o,
        else => return null,
    };
    const kv = obj.get(integration.FIELD_INTEGRATION) orelse return null;
    const ks = switch (kv) {
        .string => |s| s,
        else => return null,
    };
    return integration.idFromString(ks);
}

const std = @import("std");
const common = @import("common");
const cache = @import("cache");
const secure_memory = @import("../secrets/secure_memory.zig");
const flight = @import("broker_flight.zig");
const broker_key = @import("broker_key.zig");
const integration = @import("integration.zig");
const Spec = integration.Spec;
