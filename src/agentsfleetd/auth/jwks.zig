//! Generic JWKS-based JWT verification (RS256).
//! Works with any OIDC provider that exposes a JWKS endpoint.
//! Provider-specific claim extraction lives in claims.zig.

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const logging = @import("log");
const ec = @import("auth_codes");
const jwks_types = @import("jwks_types.zig");
const jwks_token = @import("jwks_token.zig");
const jwks_crypto = @import("jwks_crypto.zig");
const jwks_fetch = @import("jwks_fetch.zig");
const MS_PER_SECOND = 1000;

const log = logging.scoped(.auth);

pub const VerifyError = jwks_types.VerifyError;
pub const VerifiedClaims = jwks_types.VerifiedClaims;
pub const extractBearerToken = jwks_token.extractBearerToken;
pub const splitJwt = jwks_token.splitJwt;
pub const decodeBase64UrlOwned = jwks_token.decodeBase64UrlOwned;
pub const verifyRs256 = jwks_crypto.verifyRs256;
pub const parseStandardClaims = jwks_standard_claims.parseStandardClaims;
pub const getString = jwks_standard_claims.getString;

const jwks_standard_claims = @import("jwks_standard_claims.zig");

/// Minimum interval between JWKS fetch attempts, successful or not. Rate-limits
/// kid-miss-forced refreshes (key-rotation storms) and failure retries while the
/// identity provider is down; the 6h TTL refresh is always far above this.
pub const JWKS_REFRESH_MIN_INTERVAL_MS: i64 = 30 * MS_PER_SECOND;

pub const Config = struct {
    jwks_url: []const u8,
    issuer: ?[]const u8 = null,
    audience: ?[]const u8 = null,
    inline_jwks_json: ?[]const u8 = null,
    cache_ttl_ms: i64 = 6 * 60 * 60 * MS_PER_SECOND,
};

const Header = struct {
    alg: []const u8,
    kid: ?[]const u8 = null,
};

const JwkDoc = struct {
    keys: []const struct {
        kid: ?[]const u8 = null,
        kty: ?[]const u8 = null,
        n: ?[]const u8 = null,
        e: ?[]const u8 = null,
    },
};

const JwkKey = jwks_types.JwkKey;
const JwksCache = jwks_types.JwksCache;

pub const Verifier = struct {
    const Self = @This();

    alloc: std.mem.Allocator,
    jwks_url: []u8,
    issuer: ?[]u8,
    audience: ?[]u8,
    inline_jwks_json: ?[]u8,
    cache_ttl_ms: i64,
    mutex: common.Mutex = .{},
    cache: ?JwksCache = null,
    // Single-flight refresh state, all guarded by `mutex`. `refresh_fetch_count`
    // is written only by the flight leader (serialized by `refresh_inflight`).
    refresh_inflight: bool = false,
    refresh_cond: common.Condition = .{},
    last_refresh_attempt_ms: i64 = 0,
    refresh_fetch_count: u64 = 0,

    pub fn init(alloc: std.mem.Allocator, cfg: Config) error{OutOfMemory}!Verifier {
        // Boot-path OOM is an error the caller reports, never a process abort.
        const jwks_url = try alloc.dupe(u8, cfg.jwks_url);
        errdefer alloc.free(jwks_url);
        const issuer = if (cfg.issuer) |v| try alloc.dupe(u8, v) else null;
        errdefer if (issuer) |v| alloc.free(v);
        const audience = if (cfg.audience) |v| try alloc.dupe(u8, v) else null;
        errdefer if (audience) |v| alloc.free(v);
        const inline_jwks_json = if (cfg.inline_jwks_json) |v| try alloc.dupe(u8, v) else null;
        return .{
            .alloc = alloc,
            .jwks_url = jwks_url,
            .issuer = issuer,
            .audience = audience,
            .inline_jwks_json = inline_jwks_json,
            .cache_ttl_ms = cfg.cache_ttl_ms,
        };
    }

    pub fn deinit(self: *Self) void {
        self.mutex.lock();
        defer self.mutex.unlock();
        if (self.cache) |*cache| cache.deinit(self.alloc);
        self.cache = null;
        self.alloc.free(self.jwks_url);
        if (self.issuer) |v| self.alloc.free(v);
        if (self.audience) |v| self.alloc.free(v);
        if (self.inline_jwks_json) |v| self.alloc.free(v);
    }

    /// Verify JWT signature, check standard claims (sub, iss, aud, exp),
    /// return verified claims including raw JSON for provider-specific extraction.
    pub fn verifyAndDecode(self: *Self, alloc: std.mem.Allocator, authorization: []const u8) !VerifiedClaims {
        const token = extractBearerToken(authorization) catch return VerifyError.InvalidAuthorization;
        const parts = splitJwt(token) catch return VerifyError.TokenMalformed;

        log.debug("token_parsed", .{});

        const header_raw = try decodeBase64UrlOwned(alloc, parts.header_b64);
        defer alloc.free(header_raw);
        const payload_raw = try decodeBase64UrlOwned(alloc, parts.payload_b64);
        errdefer alloc.free(payload_raw);
        const signature = try decodeBase64UrlOwned(alloc, parts.signature_b64);
        defer alloc.free(signature);

        const header = try std.json.parseFromSlice(Header, alloc, header_raw, .{ .ignore_unknown_fields = true });
        defer header.deinit();
        if (!std.mem.eql(u8, header.value.alg, "RS256")) {
            log.warn("unsupported_alg", .{ .error_code = ec.ERR_UNAUTHORIZED, .alg = header.value.alg });
            return VerifyError.UnsupportedAlgorithm;
        }
        const kid = header.value.kid orelse {
            log.warn("missing_kid", .{
                .error_code = ec.ERR_UNAUTHORIZED,
            });
            return VerifyError.MissingKeyId;
        };

        log.debug("token_kid", .{ .kid = kid });

        const key = try self.lookupKey(alloc, kid);
        defer {
            alloc.free(key.kid);
            alloc.free(key.modulus);
            alloc.free(key.exponent);
        }

        const signing_input = try std.fmt.allocPrint(alloc, "{s}.{s}", .{ parts.header_b64, parts.payload_b64 });
        defer alloc.free(signing_input);

        verifyRs256(signing_input, signature, key.modulus, key.exponent) catch {
            log.warn("signature_invalid", .{ .error_code = ec.ERR_UNAUTHORIZED, .kid = kid });
            return VerifyError.SignatureInvalid;
        };

        return parseStandardClaims(alloc, payload_raw, self.issuer, self.audience);
    }

    pub fn checkJwksConnectivity(self: *Self) !void {
        try self.refreshSingleFlight(.expired);
    }

    const CacheScan = enum { fresh_only, allow_stale };
    const RefreshReason = enum { expired, kid_miss };
    const CacheLookup = union(enum) { hit: JwkKey, miss_fresh, miss_stale_or_none };

    fn lookupKey(self: *Self, alloc: std.mem.Allocator, kid: []const u8) !JwkKey {
        switch (try self.cachedKey(alloc, kid, .fresh_only)) {
            .hit => |key| return key,
            // Fresh cache without this kid: the issuer likely rotated keys —
            // force a (rate-limited) refresh before giving up. AUTH.md pins
            // this "refresh on kid miss" behaviour.
            .miss_fresh => return self.lookupAfterRefresh(alloc, kid, .kid_miss),
            .miss_stale_or_none => return self.lookupAfterRefresh(alloc, kid, .expired),
        }
    }

    fn lookupAfterRefresh(self: *Self, alloc: std.mem.Allocator, kid: []const u8, reason: RefreshReason) !JwkKey {
        if (self.refreshSingleFlight(reason)) |_| {
            switch (try self.cachedKey(alloc, kid, .allow_stale)) {
                .hit => |key| return key,
                else => return VerifyError.JwkNotFound,
            }
        } else |err| {
            // Stale-serve: a failed refresh keeps the prior key set; verifying
            // against known keys beats a hard auth outage while the identity
            // provider is unreachable.
            switch (try self.cachedKey(alloc, kid, .allow_stale)) {
                .hit => |key| {
                    log.warn("jwks_stale_serve", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
                    return key;
                },
                else => return err,
            }
        }
    }

    fn cachedKey(self: *Self, alloc: std.mem.Allocator, kid: []const u8, scan: CacheScan) !CacheLookup {
        self.mutex.lock();
        defer self.mutex.unlock();
        const cache = if (self.cache) |*c| c else return .miss_stale_or_none;
        const fresh = clock.nowMillis() - cache.fetched_at_ms <= self.cache_ttl_ms;
        if (scan == .fresh_only and !fresh) return .miss_stale_or_none;
        for (cache.keys) |key| {
            if (!std.mem.eql(u8, key.kid, kid)) continue;
            const kid_copy = try alloc.dupe(u8, key.kid);
            errdefer alloc.free(kid_copy);
            const modulus = try alloc.dupe(u8, key.modulus);
            errdefer alloc.free(modulus);
            const exponent = try alloc.dupe(u8, key.exponent);
            return .{ .hit = .{ .kid = kid_copy, .modulus = modulus, .exponent = exponent } };
        }
        return if (fresh) .miss_fresh else .miss_stale_or_none;
    }

    /// Exactly one thread fetches at a time; the others wait and then re-read
    /// the cache. The network round-trip happens with the mutex RELEASED, so
    /// cache-hit verification never blocks behind a slow identity provider.
    /// A failed fetch (or parse) leaves the previous cache in place.
    fn refreshSingleFlight(self: *Self, reason: RefreshReason) !void {
        const entered_ms = clock.nowMillis();
        self.mutex.lock();
        while (self.refresh_inflight) self.refresh_cond.wait(&self.mutex);
        const fresh = if (self.cache) |*c| clock.nowMillis() - c.fetched_at_ms <= self.cache_ttl_ms else false;
        if (reason == .expired and fresh) {
            // Another flight refreshed while we waited (or raced us to it).
            self.mutex.unlock();
            return;
        }
        if (entered_ms - self.last_refresh_attempt_ms < JWKS_REFRESH_MIN_INTERVAL_MS) {
            // Rate-limited: the caller serves whatever key set we still hold.
            self.mutex.unlock();
            return;
        }
        self.refresh_inflight = true;
        self.last_refresh_attempt_ms = entered_ms;
        self.mutex.unlock();

        const fetched = self.fetchJwksJson();

        self.mutex.lock();
        defer {
            self.refresh_inflight = false;
            self.refresh_cond.broadcast();
            self.mutex.unlock();
        }
        const raw = fetched catch |err| {
            log.warn("jwks_fetch_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
            return err;
        };
        defer self.alloc.free(raw);
        var parsed = parseJwks(self.alloc, raw) catch |err| {
            log.warn("jwks_parse_failed", .{ .error_code = ec.ERR_AUTH_UNAVAILABLE, .err = @errorName(err) });
            return err;
        };
        parsed.fetched_at_ms = clock.nowMillis();
        if (self.cache) |*old| old.deinit(self.alloc);
        self.cache = parsed;
        log.debug("jwks_fetched", .{ .keys = parsed.keys.len, .reason = @tagName(reason) });
    }

    fn fetchJwksJson(self: *Self) ![]u8 {
        self.refresh_fetch_count += 1;
        if (self.inline_jwks_json) |raw| return self.alloc.dupe(u8, raw);

        if (self.jwks_url.len == 0) return VerifyError.JwksFetchFailed;

        // Capped, redirect-following GET (jwks_fetch.zig): the identity
        // provider URL is config-controlled, so the read is bounded by a
        // named cap, never trusted.
        return jwks_fetch.fetchCapped(self.alloc, self.jwks_url) catch |err| switch (err) {
            error.OutOfMemory => error.OutOfMemory,
            else => VerifyError.JwksFetchFailed,
        };
    }
};

pub fn parseJwks(alloc: std.mem.Allocator, raw: []const u8) !JwksCache {
    // OOM is a resource failure, not a malformed key set (RULE ECL).
    const parsed = std.json.parseFromSlice(JwkDoc, alloc, raw, .{ .ignore_unknown_fields = true }) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        else => return VerifyError.JwksParseFailed,
    };
    defer parsed.deinit();

    var keys: std.ArrayList(JwkKey) = .empty;
    errdefer {
        for (keys.items) |key| {
            alloc.free(key.kid);
            alloc.free(key.modulus);
            alloc.free(key.exponent);
        }
        keys.deinit(alloc);
    }

    for (parsed.value.keys) |key| {
        if (key.kid == null or key.n == null or key.e == null) continue;
        if (key.kty) |kty| {
            if (!std.mem.eql(u8, kty, "RSA")) continue;
        }

        // Owned one at a time, each with its own rung. Built inside the
        // `append` argument list these three leaked: a decode failure on
        // `modulus` or `exponent` left `kid` allocated and unreferenced, and
        // the `errdefer` above only reaches keys already appended. The daemon
        // refetches this key set from a config-controlled provider URL for the
        // life of the process, so that leak compounds per refresh.
        const kid = try alloc.dupe(u8, key.kid.?);
        errdefer alloc.free(kid);
        const modulus = try decodeBase64UrlOwned(alloc, key.n.?);
        errdefer alloc.free(modulus);
        const exponent = try decodeBase64UrlOwned(alloc, key.e.?);
        errdefer alloc.free(exponent);

        try keys.append(alloc, .{ .kid = kid, .modulus = modulus, .exponent = exponent });
    }

    if (keys.items.len == 0) return VerifyError.JwksParseFailed;

    return .{
        .fetched_at_ms = 0,
        .keys = try keys.toOwnedSlice(alloc),
    };
}

test {
    _ = @import("./jwks_test.zig");
}
