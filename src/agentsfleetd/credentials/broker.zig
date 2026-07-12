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

/// Cache-key separator joining (workspace, integration, fingerprint). ASCII unit
/// separator — never present in any field, so key boundaries cannot collide.
const KEY_SEP: u8 = 0x1f;

/// Hex width of the fingerprint appended to the cache key.
const FP_HEX_LEN: usize = @sizeOf(u64) * 2;

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
/// Per-process Wyhash seed for the identity fingerprint. The fingerprint only
/// ever compares against itself within this broker's cache, so a random seed
/// costs nothing — and keeps handle-influenced collisions from being
/// precomputable offline.
fp_seed: u64,
/// Single-flight registry for cold-miss mints (`broker_flight.zig`): a key
/// present here is being minted right now; losers wait on the condition and
/// re-read the cache. The mutex guards `inflight` and nothing else.
inflight_mutex: common.Mutex = .{},
inflight_cond: common.Condition = .{},
inflight: std.StringHashMapUnmanaged(void) = .empty,

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
) !integration.MintResult {
    const id = parseIntegration(handle) orelse {
        self.emit(integration_id, OUTCOME_UNKNOWN, false);
        return .unknown_integration;
    };
    var key_buf: [512]u8 = undefined;
    const key = writeKey(&key_buf, workspace, @tagName(id), self.identityFingerprint(handle)) orelse return .{ .mint_failed = .permanent };

    if (self.cachedToken(alloc, key, @tagName(id), now_ms)) |res| return res;

    // Single-flight (broker_flight.zig): exactly one cold-miss mint per key.
    // A loser waits, then re-reads what the winner cached; a winner that
    // cached nothing (mint failed) frees the next waiter to take its own
    // flight through the loop. If the guard cannot be established at all
    // (allocation failure), fail closed rather than mint unguarded — a
    // concurrent unguarded mint reuses the refresh token and can cost the
    // whole token family (see beginFlight).
    var claim = flight.beginFlight(self, key);
    while (claim == .lost) {
        if (self.cachedToken(alloc, key, @tagName(id), now_ms)) |res| return res;
        claim = flight.beginFlight(self, key);
    }
    if (claim == .unavailable) {
        self.emit(@tagName(id), OUTCOME_MINT_FAILED, false);
        return .{ .mint_failed = .transient };
    }
    defer flight.endFlight(self, key);

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
    const tok = alloc.dupe(u8, minted.token) catch return .{ .mint_failed = .transient };
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
    self.emit(id_name, OUTCOME_OK, false);
    return .{ .ok = .{ .token = tok, .expires_at_ms = minted.expires_at_ms, .rotated_refresh_token = rotated } };
}

/// Fresh-enough cached token for `key`, duped into `alloc`. Null on a miss or
/// a skew-expired entry (the caller re-mints; the put overwrites).
fn cachedToken(self: *CredentialBroker, alloc: std.mem.Allocator, key: []const u8, id_name: []const u8, now_ms: i64) ?integration.MintResult {
    const entry = self.store.get(key) orelse return null;
    defer entry.release();
    if (now_ms >= entry.value.expires_at_ms - EXPIRY_SKEW_MS) return null;
    const tok = alloc.dupe(u8, entry.value.token) catch return .{ .mint_failed = .transient };
    self.emit(id_name, OUTCOME_OK, true);
    // A hit did no exchange, so rotated_refresh_token stays null.
    return .{ .ok = .{ .token = tok, .expires_at_ms = entry.value.expires_at_ms } };
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
    if (workspace.len + id_name.len + 2 + FP_HEX_LEN > buf.len) return null;
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
/// reconnect misses ONLY because at least one non-excluded field changed —
/// which the connect callbacks guarantee by stamping `connected_at_ms` on every
/// stored handle (a refresh provider's other identity fields can be constants).
/// Non-object handles (rejected upstream by `parseIntegration`) hash their
/// raw value defensively.
fn identityFingerprint(self: *const CredentialBroker, handle: std.json.Value) u64 {
    var hasher = std.hash.Wyhash.init(self.fp_seed);
    switch (handle) {
        .object => |obj| hashObject(&hasher, obj, true),
        else => hashValue(&hasher, handle),
    }
    return hasher.final();
}

/// Hash `obj` in canonical (ascending key) order via an allocation-free
/// selection walk, so JSON parser/insertion order cannot change the result.
/// Every key and string value is length-framed so adjacent fields cannot
/// alias across boundaries ({"a":"xb","c":…} vs {"a":"x","bc":…}).
/// `exclude_rotating` drops the rotating-credential fields (top level only).
fn hashObject(hasher: *std.hash.Wyhash, obj: std.json.ObjectMap, exclude_rotating: bool) void {
    var prev: ?[]const u8 = null;
    while (nextKeyAfter(obj, prev, exclude_rotating)) |key| {
        hashFramed(hasher, key);
        hashValue(hasher, obj.get(key).?);
        prev = key;
    }
}

/// Length-prefix + bytes: the injective framing for variable-length pieces.
fn hashFramed(hasher: *std.hash.Wyhash, bytes: []const u8) void {
    hasher.update(std.mem.asBytes(&bytes.len));
    hasher.update(bytes);
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
        .number_string, .string => |s| hashFramed(hasher, s),
        .array => |arr| {
            hasher.update(std.mem.asBytes(&arr.items.len));
            for (arr.items) |item| hashValue(hasher, item);
        },
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
const flight = @import("broker_flight.zig");
const integration = @import("integration.zig");
const Spec = integration.Spec;
