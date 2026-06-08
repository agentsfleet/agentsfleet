//! Generic JWKS-based JWT verification (RS256).
//! Works with any OIDC provider that exposes a JWKS endpoint.
//! Provider-specific claim extraction lives in claims.zig.

const std = @import("std");
const common = @import("common");
const clock = @import("common").clock;
const logging = @import("log");
const jwks_types = @import("jwks_types.zig");
const jwks_token = @import("jwks_token.zig");
const jwks_crypto = @import("jwks_crypto.zig");
const MS_PER_SECOND = 1000;

const log = logging.scoped(.auth);

const PANIC_OOM = "oom";

pub const VerifyError = jwks_types.VerifyError;
pub const VerifiedClaims = jwks_types.VerifiedClaims;
pub const extractBearerToken = jwks_token.extractBearerToken;
pub const splitJwt = jwks_token.splitJwt;
pub const decodeBase64UrlOwned = jwks_token.decodeBase64UrlOwned;
pub const verifyRs256 = jwks_crypto.verifyRs256;

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
    alloc: std.mem.Allocator,
    jwks_url: []u8,
    issuer: ?[]u8,
    audience: ?[]u8,
    inline_jwks_json: ?[]u8,
    cache_ttl_ms: i64,
    mutex: common.Mutex = .{},
    cache: ?JwksCache = null,

    pub fn init(alloc: std.mem.Allocator, cfg: Config) Verifier {
        return .{
            .alloc = alloc,
            .jwks_url = alloc.dupe(u8, cfg.jwks_url) catch @panic(PANIC_OOM),
            .issuer = if (cfg.issuer) |v| alloc.dupe(u8, v) catch @panic(PANIC_OOM) else null,
            .audience = if (cfg.audience) |v| alloc.dupe(u8, v) catch @panic(PANIC_OOM) else null,
            .inline_jwks_json = if (cfg.inline_jwks_json) |v| alloc.dupe(u8, v) catch @panic(PANIC_OOM) else null,
            .cache_ttl_ms = cfg.cache_ttl_ms,
        };
    }

    pub fn deinit(self: *Verifier) void {
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
    pub fn verifyAndDecode(self: *Verifier, alloc: std.mem.Allocator, authorization: []const u8) !VerifiedClaims {
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
            log.warn("unsupported_alg", .{ .alg = header.value.alg });
            return VerifyError.UnsupportedAlgorithm;
        }
        const kid = header.value.kid orelse {
            log.warn("missing_kid", .{});
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
            log.warn("signature_invalid", .{ .kid = kid });
            return VerifyError.SignatureInvalid;
        };

        return parseStandardClaims(alloc, payload_raw, self.issuer, self.audience);
    }

    pub fn checkJwksConnectivity(self: *Verifier) !void {
        self.mutex.lock();
        defer self.mutex.unlock();
        _ = try self.refreshCacheLocked();
    }

    fn lookupKey(self: *Verifier, alloc: std.mem.Allocator, kid: []const u8) !JwkKey {
        self.mutex.lock();
        defer self.mutex.unlock();
        const cache = try self.refreshCacheLocked();

        for (cache.keys) |key| {
            if (std.mem.eql(u8, key.kid, kid)) {
                return .{
                    .kid = try alloc.dupe(u8, key.kid),
                    .modulus = try alloc.dupe(u8, key.modulus),
                    .exponent = try alloc.dupe(u8, key.exponent),
                };
            }
        }

        return VerifyError.JwkNotFound;
    }

    fn refreshCacheLocked(self: *Verifier) !*JwksCache {
        const now_ms = clock.nowMillis();
        if (self.cache) |*cache| {
            if (now_ms - cache.fetched_at_ms <= self.cache_ttl_ms) {
                log.debug("jwks_cache_hit", .{ .age_ms = now_ms - cache.fetched_at_ms });
                return cache;
            }
            log.debug("jwks_cache_expired", .{});
            cache.deinit(self.alloc);
            self.cache = null;
        }

        const raw = self.fetchJwksJson() catch |err| {
            log.warn("jwks_fetch_failed", .{ .err = @errorName(err) });
            return err;
        };
        defer self.alloc.free(raw);

        var parsed = try parseJwks(self.alloc, raw);
        parsed.fetched_at_ms = now_ms;
        self.cache = parsed;
        log.info("jwks_fetched", .{ .keys = parsed.keys.len });
        return &self.cache.?;
    }

    fn fetchJwksJson(self: *Verifier) ![]u8 {
        if (self.inline_jwks_json) |raw| return self.alloc.dupe(u8, raw);

        if (self.jwks_url.len == 0) return VerifyError.JwksFetchFailed;

        // JWKS fetch is cached (TTL) so this runs rarely — a blocking one-shot
        // GET on the process-global io is appropriate.
        var client: std.http.Client = .{ .allocator = self.alloc, .io = common.globalIo() };
        defer client.deinit();

        var body: std.ArrayList(u8) = .empty;
        var aw: std.Io.Writer.Allocating = .fromArrayList(self.alloc, &body);

        const result = client.fetch(.{
            .location = .{ .url = self.jwks_url },
            .method = .GET,
            .response_writer = &aw.writer,
        }) catch return VerifyError.JwksFetchFailed;

        if (result.status != .ok) {
            const slice = aw.toOwnedSlice() catch return VerifyError.JwksFetchFailed;
            self.alloc.free(slice);
            return VerifyError.JwksFetchFailed;
        }
        return aw.toOwnedSlice() catch return VerifyError.JwksFetchFailed;
    }
};

pub fn parseJwks(alloc: std.mem.Allocator, raw: []const u8) !JwksCache {
    const parsed = std.json.parseFromSlice(JwkDoc, alloc, raw, .{ .ignore_unknown_fields = true }) catch return VerifyError.JwksParseFailed;
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

        try keys.append(alloc, .{
            .kid = try alloc.dupe(u8, key.kid.?),
            .modulus = try decodeBase64UrlOwned(alloc, key.n.?),
            .exponent = try decodeBase64UrlOwned(alloc, key.e.?),
        });
    }

    if (keys.items.len == 0) return VerifyError.JwksParseFailed;

    return .{
        .fetched_at_ms = 0,
        .keys = try keys.toOwnedSlice(alloc),
    };
}

pub fn parseStandardClaims(
    alloc: std.mem.Allocator,
    payload_raw: []u8,
    expected_issuer: ?[]const u8,
    expected_audience: ?[]const u8,
) !VerifiedClaims {
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, payload_raw, .{}) catch return VerifyError.TokenMalformed;
    defer parsed.deinit();

    if (parsed.value != .object) return VerifyError.TokenMalformed;
    const obj = parsed.value.object;

    const subject = getString(obj, "sub") orelse return VerifyError.MissingSubject;
    const issuer = getString(obj, "iss") orelse return VerifyError.MissingIssuer;

    if (expected_issuer) |want| {
        if (!std.mem.eql(u8, issuer, want)) return VerifyError.IssuerMismatch;
    }

    if (expected_audience) |want_aud| {
        if (!audienceMatches(obj, want_aud)) return VerifyError.AudienceMismatch;
    }

    const exp = getInt(obj, "exp") orelse return VerifyError.MissingExpiry;
    const now_s = clock.nowSeconds();
    if (exp <= now_s) return VerifyError.TokenExpired;

    return .{
        .subject = try alloc.dupe(u8, subject),
        .issuer = try alloc.dupe(u8, issuer),
        .claims_json = payload_raw,
    };
}

pub fn getString(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    const value = obj.get(key) orelse return null;
    return switch (value) {
        .string => |s| s,
        else => null,
    };
}

pub fn getInt(obj: std.json.ObjectMap, key: []const u8) ?i64 {
    const value = obj.get(key) orelse return null;
    return switch (value) {
        .integer => |i| i,
        else => null,
    };
}

fn audienceMatches(obj: std.json.ObjectMap, wanted: []const u8) bool {
    const aud = obj.get("aud") orelse return false;
    switch (aud) {
        .string => |value| return std.mem.eql(u8, value, wanted),
        .array => |arr| {
            for (arr.items) |item| {
                if (item == .string and std.mem.eql(u8, item.string, wanted)) return true;
            }
            return false;
        },
        else => return false,
    }
}

test {
    _ = @import("./jwks_test.zig");
}
