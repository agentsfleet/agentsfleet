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

/// Cache-key separator joining (workspace, integration, fingerprint). ASCII unit
/// separator — never present in any field, so key boundaries cannot collide.
const KEY_SEP: u8 = 0x1f;

/// Wyhash seed for the identity fingerprint. Any fixed value works — the
/// fingerprint only ever compares against itself within this process's cache.
const FP_SEED: u64 = 0;

/// Floor for the cache.zig TTL (seconds); our own `now_ms` skew check is the
/// authoritative expiry, so this is only a backstop that must stay positive.
const MIN_TTL_S: u32 = 1;

/// Ceiling for the cache.zig TTL (seconds). cache.zig stores the entry expiry as
/// `@as(u32, now_epoch_seconds) + ttl` (segment.zig) — a u32 add — so an
/// unbounded ttl (e.g. a never-expires `static` token whose remaining seconds
/// exceed `maxInt(u32)`) overflows that u32 and panics. One day is a safe
/// backstop: the broker's `now_ms` skew check at the read path is authoritative,
/// so the cache TTL only needs to be long enough to avoid needless re-mints.
const MAX_TTL_S: u32 = 24 * 60 * 60;

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
    const key = writeKey(&key_buf, workspace, @tagName(id), identityFingerprint(handle)) orelse return .{ .mint_failed = .permanent };

    if (self.store.get(key)) |entry| {
        defer entry.release();
        if (now_ms < entry.value.expires_at_ms - EXPIRY_SKEW_MS) {
            const tok = alloc.dupe(u8, entry.value.token) catch return .{ .mint_failed = .transient };
            self.emit(@tagName(id), OUTCOME_OK, true);
            // A hit did no exchange, so rotated_refresh_token stays null.
            return .{ .ok = .{ .token = tok, .expires_at_ms = entry.value.expires_at_ms } };
        }
        // present but past our skew → fall through to re-mint (put overwrites it).
    }

    switch (self.runMint(id, handle, now_ms)) {
        .ok => |minted| return self.finishColdMint(alloc, key, @tagName(id), minted, now_ms),
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

/// Cache + hand back a cold-path mint. The strategy's owned copies are freed
/// here exactly once; the caller receives independent dupes — including the
/// rotated refresh token when the exchange rotated it (RULE OWN: one free path
/// per allocation, proven leak-free under `std.testing.allocator`).
fn finishColdMint(self: *CredentialBroker, alloc: std.mem.Allocator, key: []const u8, id_name: []const u8, minted: integration.Minted, now_ms: i64) integration.MintResult {
    defer self.alloc.free(minted.token); // runMint handed us an owned copy
    defer if (minted.rotated_refresh_token) |rt| self.alloc.free(rt);
    self.cacheMinted(key, minted, now_ms);
    const tok = alloc.dupe(u8, minted.token) catch return .{ .mint_failed = .transient };
    const rotated: ?[]const u8 = if (minted.rotated_refresh_token) |rt|
        alloc.dupe(u8, rt) catch {
            alloc.free(tok); // the only owner so far — free before failing closed
            return .{ .mint_failed = .transient };
        }
    else
        null;
    self.emit(id_name, OUTCOME_OK, false);
    return .{ .ok = .{ .token = tok, .expires_at_ms = minted.expires_at_ms, .rotated_refresh_token = rotated } };
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
    // Saturating subtraction: a never-expires token uses `maxInt(i64)`, and
    // `maxInt(i64) - negative_now` would overflow a plain i64 subtraction.
    const remaining_ms = expires_at_ms -| now_ms;
    if (remaining_ms <= 0) return MIN_TTL_S;
    const secs = @divFloor(remaining_ms, 1000);
    // Clamp to MAX_TTL_S, never `maxInt(u32)`: cache.zig adds `now_epoch_seconds`
    // to this ttl in a u32, so the type max is the one value guaranteed to
    // overflow it. A bounded ceiling keeps `now + ttl` inside u32.
    if (secs >= MAX_TTL_S) return MAX_TTL_S;
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

fn writeKey(buf: []u8, workspace: []const u8, id_name: []const u8, fingerprint: u64) ?[]const u8 {
    if (workspace.len + id_name.len + 2 > buf.len) return null;
    @memcpy(buf[0..workspace.len], workspace);
    buf[workspace.len] = KEY_SEP;
    @memcpy(buf[workspace.len + 1 ..][0..id_name.len], id_name);
    var pos = workspace.len + 1 + id_name.len;
    buf[pos] = KEY_SEP;
    pos += 1;
    // Fixed-width hex keeps the key length predictable and the bytes printable.
    const fp_hex = std.fmt.bufPrint(buf[pos..], "{x:0>16}", .{fingerprint}) catch return null;
    return buf[0 .. pos + fp_hex.len];
}

/// 64-bit fingerprint of the handle's STABLE identity: every top-level field
/// except the rotating-credential set (`integration.ROTATING_CREDENTIAL_FIELDS`).
/// An ordinary refresh-token rotation keeps the fingerprint (cache hit); a
/// reconnect or re-stored credential changes a non-excluded field and misses,
/// so a stale token is structurally unreachable. Non-object handles (rejected
/// upstream by `parseIntegration`) hash their raw value defensively.
fn identityFingerprint(handle: std.json.Value) u64 {
    var hasher = std.hash.Wyhash.init(FP_SEED);
    switch (handle) {
        .object => |obj| hashObject(&hasher, obj, true),
        else => hashValue(&hasher, handle),
    }
    return hasher.final();
}

/// Hash `obj` in canonical (ascending key) order via an allocation-free
/// selection walk, so JSON parser/insertion order cannot change the result.
/// `exclude_rotating` drops the rotating-credential fields (top level only).
fn hashObject(hasher: *std.hash.Wyhash, obj: std.json.ObjectMap, exclude_rotating: bool) void {
    var prev: ?[]const u8 = null;
    while (nextKeyAfter(obj, prev, exclude_rotating)) |key| {
        hasher.update(key);
        hasher.update(&[_]u8{KEY_SEP});
        hashValue(hasher, obj.get(key).?);
        prev = key;
    }
}

/// The smallest key strictly greater than `prev` (null → the smallest key),
/// skipping excluded fields. O(n²) over a vault handle's handful of fields —
/// cheaper than allocating and sorting a key list on the mint hot path.
fn nextKeyAfter(obj: std.json.ObjectMap, prev: ?[]const u8, exclude_rotating: bool) ?[]const u8 {
    var best: ?[]const u8 = null;
    var it = obj.iterator();
    while (it.next()) |e| {
        const k = e.key_ptr.*;
        if (exclude_rotating and isRotatingField(k)) continue;
        if (prev) |p| {
            if (std.mem.order(u8, k, p) != .gt) continue;
        }
        if (best == null or std.mem.order(u8, k, best.?) == .lt) best = k;
    }
    return best;
}

fn isRotatingField(name: []const u8) bool {
    for (integration.ROTATING_CREDENTIAL_FIELDS) |f| {
        if (std.mem.eql(u8, name, f)) return true;
    }
    return false;
}

/// Hash a JSON value with a leading type tag, so `"5"` and `5` (or `null` and
/// an empty string) cannot collide. Arrays keep their order (order is
/// meaningful); nested objects re-canonicalize but never exclude (the rotating
/// exclusion applies at the handle's top level only).
fn hashValue(hasher: *std.hash.Wyhash, v: std.json.Value) void {
    hasher.update(&[_]u8{@intFromEnum(std.meta.activeTag(v))});
    switch (v) {
        .null => {},
        .bool => |b| hasher.update(&[_]u8{@intFromBool(b)}),
        .integer => |n| hasher.update(std.mem.asBytes(&n)),
        .float => |f| hasher.update(std.mem.asBytes(&f)),
        .number_string, .string => |s| hasher.update(s),
        .array => |arr| for (arr.items) |item| hashValue(hasher, item),
        .object => |obj| hashObject(hasher, obj, false),
    }
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
