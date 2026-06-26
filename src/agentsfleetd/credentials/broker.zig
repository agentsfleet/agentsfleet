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
//! Tradeoff vs the prior hand-rolled version: cache.zig's get/put does not
//! single-flight, so two simultaneous cold-misses for the SAME key may both mint
//! (rare; harmless — GitHub returns valid tokens, the last put wins). If that ever
//! bites a rate limit, a thin in-flight guard layers on top without touching this.

const CredentialBroker = @This();

/// Re-mint this many ms BEFORE the upstream expiry so a token handed to a tool
/// call has slack to complete (RULE UFS).
pub const EXPIRY_SKEW_MS: i64 = 60_000;

/// Cache shape: 64 segments (independent RwLocks) bounds cross-workspace
/// contention; the size cap bounds memory (LRU evicts the rest).
const CACHE_SEGMENTS: u16 = 64;
const CACHE_MAX_ENTRIES: u32 = 8192;

/// Cache-key separator joining (workspace, integration). ASCII unit separator —
/// never present in either field, so key boundaries cannot collide.
const KEY_SEP: u8 = 0x1f;

/// Floor for the cache.zig TTL (seconds); our own `now_ms` skew check is the
/// authoritative expiry, so this is only a backstop that must stay positive.
const MIN_TTL_S: u32 = 1;

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
        allocator.free(self.token);
    }
};

const TokenCache = cache.Cache(TokenVal);

alloc: std.mem.Allocator,
registry: []const Spec,
deps: integration.Deps,
store: TokenCache,

/// `registry` is injected (production passes `integration.REGISTRY`) so a test can
/// supply a fake-id registry and prove dispatch is data-driven. `deps` carries the
/// daemon-singleton effects (the App key loaded ONCE, the HTTP boundary, the RS256
/// signer, the metrics hook) folded into every `MintCtx`.
pub fn init(alloc: std.mem.Allocator, registry: []const Spec, deps: integration.Deps) !CredentialBroker {
    return .{
        .alloc = alloc,
        .registry = registry,
        .deps = deps,
        .store = try TokenCache.init(common.globalIo(), alloc, .{
            .segment_count = CACHE_SEGMENTS,
            .max_size = CACHE_MAX_ENTRIES,
        }),
    };
}

pub fn deinit(self: *CredentialBroker) void {
    self.store.deinit();
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
) !integration.MintResult {
    const id = parseIntegration(handle) orelse {
        self.emit(integration_id, OUTCOME_UNKNOWN, false);
        return .unknown_integration;
    };
    var key_buf: [512]u8 = undefined;
    const key = writeKey(&key_buf, workspace, @tagName(id)) orelse return .{ .mint_failed = .permanent };

    if (self.store.get(key)) |entry| {
        defer entry.release();
        if (now_ms < entry.value.expires_at_ms - EXPIRY_SKEW_MS) {
            const tok = alloc.dupe(u8, entry.value.token) catch return .{ .mint_failed = .transient };
            self.emit(@tagName(id), OUTCOME_OK, true);
            return .{ .ok = .{ .token = tok, .expires_at_ms = entry.value.expires_at_ms } };
        }
        // present but past our skew → fall through to re-mint (put overwrites it).
    }

    const outcome = self.runMint(id, handle, now_ms);
    switch (outcome) {
        .ok => |minted| {
            defer self.alloc.free(minted.token); // runMint handed us an owned copy
            self.cacheMinted(key, minted, now_ms);
            const tok = alloc.dupe(u8, minted.token) catch return .{ .mint_failed = .transient };
            self.emit(@tagName(id), OUTCOME_OK, false);
            return .{ .ok = .{ .token = tok, .expires_at_ms = minted.expires_at_ms } };
        },
        .reconnect_required => {
            self.emit(@tagName(id), OUTCOME_RECONNECT, false);
            return .reconnect_required;
        },
        .mint_failed => |retry| {
            self.emit(@tagName(id), OUTCOME_MINT_FAILED, false);
            return .{ .mint_failed = retry };
        },
    }
}

/// Store a freshly-minted token (cache.zig owns the duped bytes; frees via
/// `removedFromCache` on eviction). A put failure is non-fatal — the caller still
/// gets its token, just without a cache entry.
fn cacheMinted(self: *CredentialBroker, key: []const u8, minted: integration.Minted, now_ms: i64) void {
    const owned = self.alloc.dupe(u8, minted.token) catch return;
    self.store.put(key, .{ .token = owned, .expires_at_ms = minted.expires_at_ms }, .{
        .ttl = ttlSeconds(minted.expires_at_ms, now_ms),
    }) catch self.alloc.free(owned);
}

/// cache.zig expiry backstop (seconds). Our `now_ms` skew check is authoritative;
/// in production `now_ms` tracks wall time so this matches the real remaining life.
fn ttlSeconds(expires_at_ms: i64, now_ms: i64) u32 {
    const remaining_ms = expires_at_ms - now_ms;
    if (remaining_ms <= 0) return MIN_TTL_S;
    const secs = @divFloor(remaining_ms, 1000);
    if (secs > std.math.maxInt(u32)) return std.math.maxInt(u32);
    return @max(MIN_TTL_S, @as(u32, @intCast(secs)));
}

/// Dispatch to the integration's mint with a fully-built `MintCtx`. Runs WITHOUT
/// any cache lock held (the network call must not serialize other minters).
fn runMint(self: *CredentialBroker, id: integration.Id, handle: std.json.Value, now_ms: i64) integration.Outcome {
    const spec = integration.resolve(self.registry, id) orelse return .{ .mint_failed = .permanent };
    const ctx = integration.MintCtx{
        .alloc = self.alloc,
        .handle = handle,
        .now_ms = now_ms,
        .platform = self.deps.platform,
        .http = self.deps.http,
        .sign = self.deps.sign,
    };
    // The strategy union owns dispatch; the broker never branches on id.
    return spec.mint.run(ctx) catch .{ .mint_failed = .transient };
}

fn emit(self: *CredentialBroker, integration_name: []const u8, outcome: []const u8, cache_hit: bool) void {
    self.deps.metrics.onMint(.{
        .integration = integration_name,
        .outcome = outcome,
        .latency_ms = 0,
        .cache_hit = cache_hit,
    });
}

fn writeKey(buf: []u8, workspace: []const u8, id_name: []const u8) ?[]const u8 {
    if (workspace.len + id_name.len + 1 > buf.len) return null;
    @memcpy(buf[0..workspace.len], workspace);
    buf[workspace.len] = KEY_SEP;
    @memcpy(buf[workspace.len + 1 ..][0..id_name.len], id_name);
    return buf[0 .. workspace.len + 1 + id_name.len];
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
const integration = @import("integration.zig");
const Spec = integration.Spec;
