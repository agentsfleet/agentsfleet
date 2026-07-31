const std = @import("std");
const jwks = @import("jwks.zig");
const common = @import("common");
const clock = common.clock;

const VerifyError = jwks.VerifyError;
const Verifier = jwks.Verifier;
const VerifiedClaims = jwks.VerifiedClaims;
const extractBearerToken = jwks.extractBearerToken;
const splitJwt = jwks.splitJwt;
const decodeBase64UrlOwned = jwks.decodeBase64UrlOwned;
const parseJwks = jwks.parseJwks;
const verifyRs256 = jwks.verifyRs256;
const parseStandardClaims = jwks.parseStandardClaims;
// ── Test fixtures ──────────────────────────────────────────────────────

// Single-sourced in jwks_test_fixtures.zig (Dimension 6.4); aliased so the
// call sites below stay unchanged.
const fx = @import("jwks_test_fixtures.zig");
const TEST_JWKS = fx.TEST_JWKS;
const TEST_VALID_TOKEN = fx.TEST_VALID_TOKEN;
const TEST_EXPIRED_TOKEN = fx.TEST_EXPIRED_TOKEN;
const WRONG_KID_JWKS = fx.WRONG_KID_JWKS;

fn makeTestVerifier(inline_jwks: ?[]const u8) error{OutOfMemory}!Verifier {
    return Verifier.init(std.testing.allocator, .{
        .jwks_url = "https://clerk.dev.agentsfleet.net/.well-known/jwks.json",
        .issuer = "https://clerk.dev.agentsfleet.net",
        .audience = "https://api.agentsfleet.net",
        .inline_jwks_json = inline_jwks orelse TEST_JWKS,
    });
}

fn freeClaims(vc: VerifiedClaims) void {
    std.testing.allocator.free(vc.subject);
    std.testing.allocator.free(vc.issuer);
    std.testing.allocator.free(vc.claims_json);
}

// ── Bearer extraction tests ────────────────────────────────────────────

test "extractBearerToken: valid bearer" {
    const t = try extractBearerToken("Bearer abc123");
    try std.testing.expectEqualStrings("abc123", t);
}

test "extractBearerToken: missing prefix" {
    try std.testing.expectError(VerifyError.InvalidAuthorization, extractBearerToken("Basic abc123"));
}

test "extractBearerToken: empty token after prefix" {
    try std.testing.expectError(VerifyError.InvalidAuthorization, extractBearerToken("Bearer    "));
}

test "extractBearerToken: lowercase bearer rejected" {
    try std.testing.expectError(VerifyError.InvalidAuthorization, extractBearerToken("bearer abc123"));
}

test "extractBearerToken: empty string" {
    try std.testing.expectError(VerifyError.InvalidAuthorization, extractBearerToken(""));
}

// ── JWT splitting tests ────────────────────────────────────────────────

test "splitJwt: valid three parts" {
    const parts = try splitJwt("aaa.bbb.ccc");
    try std.testing.expectEqualStrings("aaa", parts.header_b64);
    try std.testing.expectEqualStrings("bbb", parts.payload_b64);
    try std.testing.expectEqualStrings("ccc", parts.signature_b64);
}

test "splitJwt: too few parts" {
    try std.testing.expectError(VerifyError.TokenMalformed, splitJwt("aaa.bbb"));
}

test "splitJwt: too many parts" {
    try std.testing.expectError(VerifyError.TokenMalformed, splitJwt("a.b.c.d"));
}

test "splitJwt: empty segment" {
    try std.testing.expectError(VerifyError.TokenMalformed, splitJwt("aaa..ccc"));
}

test "splitJwt: single dot" {
    try std.testing.expectError(VerifyError.TokenMalformed, splitJwt("."));
}

// ── JWKS parsing tests ────────────────────────────────────────────────

test "parseJwks: valid single RSA key" {
    var cache = try parseJwks(std.testing.allocator, TEST_JWKS);
    defer cache.deinit(std.testing.allocator);
    try std.testing.expectEqual(@as(usize, 1), cache.keys.len);
    try std.testing.expectEqualStrings("test-kid-static", cache.keys[0].kid);
}

test "parseJwks: empty keys array" {
    try std.testing.expectError(VerifyError.JwksParseFailed, parseJwks(std.testing.allocator,
        \\{"keys":[]}
    ));
}

test "parseJwks: not valid JSON" {
    try std.testing.expectError(VerifyError.JwksParseFailed, parseJwks(std.testing.allocator, "not json at all"));
}

test "parseJwks: key missing n field is skipped" {
    try std.testing.expectError(VerifyError.JwksParseFailed, parseJwks(std.testing.allocator,
        \\{"keys":[{"kty":"RSA","kid":"k1","e":"AQAB"}]}
    ));
}

test "parseJwks: key missing kid is skipped" {
    const missing_kid = "{\"keys\":[{\"kty\":\"RSA\",\"n\":\"" ++ fx.TEST_RSA_N ++ "\",\"e\":\"AQAB\"}]}";
    try std.testing.expectError(VerifyError.JwksParseFailed, parseJwks(std.testing.allocator, missing_kid));
}

test "parseJwks: non-RSA key is skipped" {
    const ec_key = "{\"keys\":[{\"kty\":\"EC\",\"kid\":\"ec1\",\"n\":\"" ++ fx.TEST_RSA_N ++ "\",\"e\":\"AQAB\"}]}";
    try std.testing.expectError(VerifyError.JwksParseFailed, parseJwks(std.testing.allocator, ec_key));
}

// ── Full verifyAndDecode edge cases ────────────────────────────────────

test "verifyAndDecode: valid token" {
    var v = try makeTestVerifier(null);
    defer v.deinit();
    const vc = try v.verifyAndDecode(std.testing.allocator, "Bearer " ++ TEST_VALID_TOKEN);
    defer freeClaims(vc);
    try std.testing.expectEqualStrings("user_test", vc.subject);
    try std.testing.expectEqualStrings("https://clerk.dev.agentsfleet.net", vc.issuer);
}

test "verifyAndDecode: expired token" {
    var v = try makeTestVerifier(null);
    defer v.deinit();
    try std.testing.expectError(VerifyError.TokenExpired, v.verifyAndDecode(std.testing.allocator, "Bearer " ++ TEST_EXPIRED_TOKEN));
}

test "verifyAndDecode: missing Authorization header" {
    var v = try makeTestVerifier(null);
    defer v.deinit();
    try std.testing.expectError(VerifyError.InvalidAuthorization, v.verifyAndDecode(std.testing.allocator, ""));
}

test "verifyAndDecode: garbage token" {
    var v = try makeTestVerifier(null);
    defer v.deinit();
    try std.testing.expectError(VerifyError.InvalidAuthorization, v.verifyAndDecode(std.testing.allocator, "not-a-bearer"));
}

test "verifyAndDecode: Bearer with no token" {
    var v = try makeTestVerifier(null);
    defer v.deinit();
    try std.testing.expectError(VerifyError.InvalidAuthorization, v.verifyAndDecode(std.testing.allocator, "Bearer "));
}

test "verifyAndDecode: token with wrong kid" {
    // The forced kid-miss refresh re-reads the same wrong-kid key set, so the
    // outcome is still JwkNotFound — pins that a refresh is not a free pass.
    var v = try makeTestVerifier(WRONG_KID_JWKS);
    defer v.deinit();
    try std.testing.expectError(VerifyError.JwkNotFound, v.verifyAndDecode(std.testing.allocator, "Bearer " ++ TEST_VALID_TOKEN));
}

test "verifyAndDecode: kid miss on fresh cache forces refresh (key rotation)" {
    const alloc = std.testing.allocator;
    var v = try makeTestVerifier(WRONG_KID_JWKS);
    defer v.deinit();
    // Prime the cache with the pre-rotation key set (token's kid absent).
    try v.checkJwksConnectivity();
    // Rotation: the endpoint now serves the new key set.
    alloc.free(v.inline_jwks_json.?);
    v.inline_jwks_json = try alloc.dupe(u8, TEST_JWKS);
    v.last_refresh_attempt_ms = 0; // outside the rate-limit window
    const vc = try v.verifyAndDecode(alloc, "Bearer " ++ TEST_VALID_TOKEN);
    defer freeClaims(vc);
    try std.testing.expectEqualStrings("user_test", vc.subject);
}

test "verifyAndDecode: kid-miss refresh is rate-limited within the window" {
    const alloc = std.testing.allocator;
    var v = try makeTestVerifier(WRONG_KID_JWKS);
    defer v.deinit();
    try v.checkJwksConnectivity();
    alloc.free(v.inline_jwks_json.?);
    v.inline_jwks_json = try alloc.dupe(u8, TEST_JWKS);
    // Last attempt just happened: a kid-miss storm must not refetch.
    v.last_refresh_attempt_ms = clock.nowMillis();
    try std.testing.expectError(VerifyError.JwkNotFound, v.verifyAndDecode(alloc, "Bearer " ++ TEST_VALID_TOKEN));
    try std.testing.expectEqual(@as(u64, 1), v.refresh_fetch_count); // priming fetch only
    // Past the window the refresh fires and the rotated key verifies.
    v.last_refresh_attempt_ms = clock.nowMillis() - jwks.JWKS_REFRESH_MIN_INTERVAL_MS - 1;
    const vc = try v.verifyAndDecode(alloc, "Bearer " ++ TEST_VALID_TOKEN);
    defer freeClaims(vc);
    try std.testing.expectEqual(@as(u64, 2), v.refresh_fetch_count);
}

test "verifyAndDecode: failed refresh serves stale keys (identity provider down)" {
    const alloc = std.testing.allocator;
    var v = try makeTestVerifier(null);
    defer v.deinit();
    try v.checkJwksConnectivity();
    // Identity provider goes dark: no inline fixture, no fetchable URL.
    alloc.free(v.inline_jwks_json.?);
    v.inline_jwks_json = null;
    alloc.free(v.jwks_url);
    v.jwks_url = try alloc.dupe(u8, "");
    // Cache expired and the refresh attempt is eligible — it fires and fails.
    v.cache.?.fetched_at_ms = 0;
    v.last_refresh_attempt_ms = 0;
    const vc = try v.verifyAndDecode(alloc, "Bearer " ++ TEST_VALID_TOKEN);
    defer freeClaims(vc);
    try std.testing.expectEqualStrings("user_test", vc.subject); // stale-served
}

test "verifyAndDecode: no cache and unreachable endpoint fails closed" {
    const alloc = std.testing.allocator;
    var v = try Verifier.init(alloc, .{ .jwks_url = "" });
    defer v.deinit();
    try std.testing.expectError(VerifyError.JwksFetchFailed, v.verifyAndDecode(alloc, "Bearer " ++ TEST_VALID_TOKEN));
}

test "concurrent verifies on a cold cache fetch exactly once (single-flight)" {
    var v = try makeTestVerifier(null);
    defer v.deinit();

    const THREADS = 8;
    var oks = std.atomic.Value(u32).init(0);
    const Worker = struct {
        fn run(ver: *Verifier, ok_count: *std.atomic.Value(u32)) void {
            var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
            defer arena.deinit();
            _ = ver.verifyAndDecode(arena.allocator(), "Bearer " ++ TEST_VALID_TOKEN) catch return;
            // safe because: independent success tally; no ordering required.
            _ = ok_count.fetchAdd(1, .monotonic);
        }
    };

    var threads: [THREADS]std.Thread = undefined;
    for (&threads) |*t| t.* = try std.Thread.spawn(.{}, Worker.run, .{ &v, &oks });
    for (&threads) |*t| t.join();

    try std.testing.expectEqual(@as(u32, THREADS), oks.load(.acquire));
    try std.testing.expectEqual(@as(u64, 1), v.refresh_fetch_count);
}

test "verifyAndDecode: audience mismatch" {
    var v = try Verifier.init(std.testing.allocator, .{
        .jwks_url = "https://clerk.dev.agentsfleet.net/.well-known/jwks.json",
        .issuer = "https://clerk.dev.agentsfleet.net",
        .audience = "https://wrong-audience.example.com",
        .inline_jwks_json = TEST_JWKS,
    });
    defer v.deinit();
    try std.testing.expectError(VerifyError.AudienceMismatch, v.verifyAndDecode(std.testing.allocator, "Bearer " ++ TEST_VALID_TOKEN));
}

test "verifyAndDecode: issuer mismatch" {
    var v = try Verifier.init(std.testing.allocator, .{
        .jwks_url = "https://clerk.dev.agentsfleet.net/.well-known/jwks.json",
        .issuer = "https://wrong-issuer.example.com",
        .audience = "https://api.agentsfleet.net",
        .inline_jwks_json = TEST_JWKS,
    });
    defer v.deinit();
    try std.testing.expectError(VerifyError.IssuerMismatch, v.verifyAndDecode(std.testing.allocator, "Bearer " ++ TEST_VALID_TOKEN));
}

test "verifyAndDecode: tampered payload (signature invalid)" {
    // Take valid token, modify one char in the payload segment
    const tampered = fx.TEST_HEADER ++ "." ++ "eXJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJpYXQiOjE3MDQwNjcyMDAsIm9yZ19pZCI6Im9yZ18xIiwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoidGVuYW50X2EifSwiZXhwIjo0MTAyNDQ0ODAwfQ" ++ "." ++ fx.TEST_SIG_VALID;
    var v = try makeTestVerifier(null);
    defer v.deinit();
    const result = v.verifyAndDecode(std.testing.allocator, "Bearer " ++ tampered);
    // Could be SignatureInvalid or TokenMalformed depending on what the tampered base64 decodes to
    try std.testing.expect(std.meta.isError(result));
}

test "verifyAndDecode: token with only two segments" {
    var v = try makeTestVerifier(null);
    defer v.deinit();
    try std.testing.expectError(VerifyError.TokenMalformed, v.verifyAndDecode(std.testing.allocator, "Bearer aaa.bbb"));
}

// ── Base64 URL decoding edge cases ─────────────────────────────────────

test "decodeBase64UrlOwned: valid base64url" {
    const decoded = try decodeBase64UrlOwned(std.testing.allocator, "SGVsbG8");
    defer std.testing.allocator.free(decoded);
    try std.testing.expectEqualStrings("Hello", decoded);
}

test "decodeBase64UrlOwned: invalid characters" {
    try std.testing.expectError(VerifyError.TokenMalformed, decodeBase64UrlOwned(std.testing.allocator, "!!!invalid!!!"));
}

// ── Inline JWKS and env var source tests ───────────────────────────────

test "verifier uses inline JWKS over URL" {
    var v = try makeTestVerifier(null);
    defer v.deinit();
    // If inline JWKS works, we can verify without network. This is the happy path.
    const vc = try v.verifyAndDecode(std.testing.allocator, "Bearer " ++ TEST_VALID_TOKEN);
    defer freeClaims(vc);
    try std.testing.expectEqualStrings("user_test", vc.subject);
}

test "verifier with empty URL and no inline JWKS fails" {
    var v = try Verifier.init(std.testing.allocator, .{
        .jwks_url = "",
        .issuer = "https://clerk.dev.agentsfleet.net",
    });
    defer v.deinit();
    try std.testing.expectError(VerifyError.JwksFetchFailed, v.checkJwksConnectivity());
}

// ── OWASP JWT attack vectors ──────────────────────────────────────────

// CVE-2015-9235: alg:none attack — attacker strips signature and sets alg to "none"
test "OWASP: alg:none attack rejected" {
    // {"alg":"none","typ":"JWT"} => base64url
    const header_none = "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0";
    // {"sub":"admin","iss":"https://clerk.dev.agentsfleet.net","aud":"https://api.agentsfleet.net","exp":4102444800}
    const payload = "eyJzdWIiOiJhZG1pbiIsImlzcyI6Imh0dHBzOi8vY2xlcmsuZGV2LmFnZW50c2ZsZWV0Lm5ldCIsImF1ZCI6Imh0dHBzOi8vYXBpLmFnZW50c2ZsZWV0Lm5ldCIsImV4cCI6NDEwMjQ0NDgwMH0";
    var v = try makeTestVerifier(null);
    defer v.deinit();
    // alg:none with empty signature
    try std.testing.expectError(VerifyError.UnsupportedAlgorithm, v.verifyAndDecode(
        std.testing.allocator,
        "Bearer " ++ header_none ++ "." ++ payload ++ ".e30",
    ));
}

// CVE-2016-5431: alg switching — attacker changes RS256 to HS256, signs with public key
test "OWASP: alg:HS256 switching attack rejected" {
    // {"alg":"HS256","typ":"JWT","kid":"test-kid-static"}
    const header_hs = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9";
    const payload = "eyJzdWIiOiJhZG1pbiIsImlzcyI6Imh0dHBzOi8vY2xlcmsuZGV2LmFnZW50c2ZsZWV0Lm5ldCIsImF1ZCI6Imh0dHBzOi8vYXBpLmFnZW50c2ZsZWV0Lm5ldCIsImV4cCI6NDEwMjQ0NDgwMH0";
    var v = try makeTestVerifier(null);
    defer v.deinit();
    try std.testing.expectError(VerifyError.UnsupportedAlgorithm, v.verifyAndDecode(
        std.testing.allocator,
        "Bearer " ++ header_hs ++ "." ++ payload ++ ".fakesig",
    ));
}

// alg:none with kid present — should still reject (may be TokenMalformed
// if empty sig segment is caught first, or UnsupportedAlgorithm)
test "OWASP: alg:none with kid still rejected" {
    // {"alg":"none","kid":"test-kid-static"}
    const header = "eyJhbGciOiJub25lIiwia2lkIjoidGVzdC1raWQtc3RhdGljIn0";
    const payload = "eyJzdWIiOiJ1c2VyIiwiaXNzIjoiaHR0cHM6Ly9jbGVyay5kZXYuYWdlbnRzZmxlZXQubmV0IiwiZXhwIjo0MTAyNDQ0ODAwfQ";
    var v = try makeTestVerifier(null);
    defer v.deinit();
    // Empty signature segment "." is rejected by splitJwt before alg check
    const result = v.verifyAndDecode(
        std.testing.allocator,
        "Bearer " ++ header ++ "." ++ payload ++ ".",
    );
    try std.testing.expect(std.meta.isError(result));
    // With a non-empty fake sig, we get UnsupportedAlgorithm
    try std.testing.expectError(VerifyError.UnsupportedAlgorithm, v.verifyAndDecode(
        std.testing.allocator,
        "Bearer " ++ header ++ "." ++ payload ++ ".ZmFrZQ",
    ));
}

// ── Missing required claims (parseStandardClaims) ─────────────────────

test "parseStandardClaims: missing sub" {
    const buf = std.testing.allocator.dupe(u8,
        \\{"iss":"https://example.com","exp":4102444800}
    ) catch unreachable;
    defer std.testing.allocator.free(buf);
    try std.testing.expectError(VerifyError.MissingSubject, parseStandardClaims(std.testing.allocator, buf, null, null));
}

test "parseStandardClaims: missing iss" {
    const buf = std.testing.allocator.dupe(u8,
        \\{"sub":"user_1","exp":4102444800}
    ) catch unreachable;
    defer std.testing.allocator.free(buf);
    try std.testing.expectError(VerifyError.MissingIssuer, parseStandardClaims(std.testing.allocator, buf, null, null));
}

test "parseStandardClaims: missing exp" {
    const buf = std.testing.allocator.dupe(u8,
        \\{"sub":"user_1","iss":"https://example.com"}
    ) catch unreachable;
    defer std.testing.allocator.free(buf);
    try std.testing.expectError(VerifyError.MissingExpiry, parseStandardClaims(std.testing.allocator, buf, null, null));
}

test "parseStandardClaims: exp is string not integer" {
    const buf = std.testing.allocator.dupe(u8,
        \\{"sub":"user_1","iss":"https://example.com","exp":"not-a-number"}
    ) catch unreachable;
    defer std.testing.allocator.free(buf);
    try std.testing.expectError(VerifyError.MissingExpiry, parseStandardClaims(std.testing.allocator, buf, null, null));
}

test "parseStandardClaims: sub is integer not string" {
    const buf = std.testing.allocator.dupe(u8,
        \\{"sub":12345,"iss":"https://example.com","exp":4102444800}
    ) catch unreachable;
    defer std.testing.allocator.free(buf);
    try std.testing.expectError(VerifyError.MissingSubject, parseStandardClaims(std.testing.allocator, buf, null, null));
}

test "parseStandardClaims: payload is JSON array not object" {
    const buf = std.testing.allocator.dupe(u8, "[1,2,3]") catch unreachable;
    defer std.testing.allocator.free(buf);
    try std.testing.expectError(VerifyError.TokenMalformed, parseStandardClaims(std.testing.allocator, buf, null, null));
}

test "parseStandardClaims: payload is empty JSON object" {
    const buf = std.testing.allocator.dupe(u8, "{}") catch unreachable;
    defer std.testing.allocator.free(buf);
    try std.testing.expectError(VerifyError.MissingSubject, parseStandardClaims(std.testing.allocator, buf, null, null));
}

test "parseStandardClaims: payload is not JSON" {
    const buf = std.testing.allocator.dupe(u8, "this is not json") catch unreachable;
    defer std.testing.allocator.free(buf);
    try std.testing.expectError(VerifyError.TokenMalformed, parseStandardClaims(std.testing.allocator, buf, null, null));
}

// exp boundary: exp=0 (epoch, always expired)
test "parseStandardClaims: exp at epoch is expired" {
    const buf = std.testing.allocator.dupe(u8,
        \\{"sub":"user_1","iss":"https://example.com","exp":0}
    ) catch unreachable;
    defer std.testing.allocator.free(buf);
    try std.testing.expectError(VerifyError.TokenExpired, parseStandardClaims(std.testing.allocator, buf, null, null));
}

// exp boundary: negative exp
test "parseStandardClaims: negative exp is expired" {
    const buf = std.testing.allocator.dupe(u8,
        \\{"sub":"user_1","iss":"https://example.com","exp":-1}
    ) catch unreachable;
    defer std.testing.allocator.free(buf);
    try std.testing.expectError(VerifyError.TokenExpired, parseStandardClaims(std.testing.allocator, buf, null, null));
}

// ── Audience matching edge cases ──────────────────────────────────────

test "parseStandardClaims: aud as array with match" {
    const buf = std.testing.allocator.dupe(u8,
        \\{"sub":"u","iss":"https://example.com","exp":4102444800,"aud":["https://api.example.com","https://other.example.com"]}
    ) catch unreachable;
    defer std.testing.allocator.free(buf);
    const vc = try parseStandardClaims(std.testing.allocator, buf, null, "https://api.example.com");
    std.testing.allocator.free(vc.subject);
    std.testing.allocator.free(vc.issuer);
}

test "parseStandardClaims: aud as array without match" {
    const buf = std.testing.allocator.dupe(u8,
        \\{"sub":"u","iss":"https://example.com","exp":4102444800,"aud":["https://other.example.com"]}
    ) catch unreachable;
    defer std.testing.allocator.free(buf);
    try std.testing.expectError(VerifyError.AudienceMismatch, parseStandardClaims(std.testing.allocator, buf, null, "https://api.example.com"));
}

test "parseStandardClaims: aud as empty array" {
    const buf = std.testing.allocator.dupe(u8,
        \\{"sub":"u","iss":"https://example.com","exp":4102444800,"aud":[]}
    ) catch unreachable;
    defer std.testing.allocator.free(buf);
    try std.testing.expectError(VerifyError.AudienceMismatch, parseStandardClaims(std.testing.allocator, buf, null, "https://api.example.com"));
}

test "parseStandardClaims: aud is integer (wrong type)" {
    const buf = std.testing.allocator.dupe(u8,
        \\{"sub":"u","iss":"https://example.com","exp":4102444800,"aud":12345}
    ) catch unreachable;
    defer std.testing.allocator.free(buf);
    try std.testing.expectError(VerifyError.AudienceMismatch, parseStandardClaims(std.testing.allocator, buf, null, "https://api.example.com"));
}

test "parseStandardClaims: no aud field when audience check required" {
    const buf = std.testing.allocator.dupe(u8,
        \\{"sub":"u","iss":"https://example.com","exp":4102444800}
    ) catch unreachable;
    defer std.testing.allocator.free(buf);
    try std.testing.expectError(VerifyError.AudienceMismatch, parseStandardClaims(std.testing.allocator, buf, null, "https://api.example.com"));
}

// ── Injection payloads in claim values ────────────────────────────────
// These verify that malicious claim values don't crash the parser
// and are passed through as opaque strings (defense-in-depth).

test "parseStandardClaims: SQL injection in sub" {
    const buf = std.testing.allocator.dupe(u8,
        \\{"sub":"'; DROP TABLE users; --","iss":"https://example.com","exp":4102444800}
    ) catch unreachable;
    defer std.testing.allocator.free(buf);
    const vc = try parseStandardClaims(std.testing.allocator, buf, null, null);
    defer {
        std.testing.allocator.free(vc.subject);
        std.testing.allocator.free(vc.issuer);
    }
    // Value passes through — parameterized queries at DB layer prevent injection
    try std.testing.expectEqualStrings("'; DROP TABLE users; --", vc.subject);
}

test "parseStandardClaims: XSS in sub" {
    const buf = std.testing.allocator.dupe(u8,
        \\{"sub":"<script>alert('xss')</script>","iss":"https://example.com","exp":4102444800}
    ) catch unreachable;
    defer std.testing.allocator.free(buf);
    const vc = try parseStandardClaims(std.testing.allocator, buf, null, null);
    defer {
        std.testing.allocator.free(vc.subject);
        std.testing.allocator.free(vc.issuer);
    }
    try std.testing.expectEqualStrings("<script>alert('xss')</script>", vc.subject);
}

test "parseStandardClaims: null bytes in sub" {
    const buf = std.testing.allocator.dupe(u8,
        \\{"sub":"user\u0000admin","iss":"https://example.com","exp":4102444800}
    ) catch unreachable;
    defer std.testing.allocator.free(buf);
    // Zig JSON parser treats \u0000 as a valid character in strings
    const vc = parseStandardClaims(std.testing.allocator, buf, null, null) catch |err| {
        // If the parser rejects it, that's also acceptable
        try std.testing.expect(err == VerifyError.TokenMalformed);
        return;
    };
    std.testing.allocator.free(vc.subject);
    std.testing.allocator.free(vc.issuer);
}

test "parseStandardClaims: very long sub (DoS attempt)" {
    // 10KB subject — should not crash or OOM the test allocator
    const long_sub = "A" ** 10240;
    const json = "{\"sub\":\"" ++ long_sub ++ "\",\"iss\":\"https://example.com\",\"exp\":4102444800}";
    const buf = std.testing.allocator.dupe(u8, json) catch unreachable;
    defer std.testing.allocator.free(buf);
    const vc = try parseStandardClaims(std.testing.allocator, buf, null, null);
    defer {
        std.testing.allocator.free(vc.subject);
        std.testing.allocator.free(vc.issuer);
    }
    try std.testing.expectEqual(@as(usize, 10240), vc.subject.len);
}

// ── JWKS key material attack vectors ──────────────────────────────────

test "parseJwks: truncated JSON" {
    try std.testing.expectError(VerifyError.JwksParseFailed, parseJwks(std.testing.allocator,
        \\{"keys":[{"kty":"RSA","kid":"k1","n":"AQ
    ));
}

test "parseJwks: key with empty string modulus parses but verify rejects" {
    // n="" is valid JSON, base64 decodes to 0 bytes — parses but RSA verify will reject
    var cache = try parseJwks(std.testing.allocator,
        \\{"keys":[{"kty":"RSA","kid":"k1","n":"","e":"AQAB"}]}
    );
    defer cache.deinit(std.testing.allocator);
    try std.testing.expectEqual(@as(usize, 1), cache.keys.len);
    try std.testing.expectEqual(@as(usize, 0), cache.keys[0].modulus.len);
    // 0-byte modulus → verifyRs256 rejects with SignatureInvalid
    try std.testing.expectError(VerifyError.SignatureInvalid, verifyRs256("msg", "sig", cache.keys[0].modulus, cache.keys[0].exponent));
}

test "parseJwks: JWKS with null instead of keys array" {
    try std.testing.expectError(VerifyError.JwksParseFailed, parseJwks(std.testing.allocator,
        \\{"keys":null}
    ));
}

test "parseJwks: JWKS with string instead of keys array" {
    try std.testing.expectError(VerifyError.JwksParseFailed, parseJwks(std.testing.allocator,
        \\{"keys":"not-an-array"}
    ));
}

test "parseJwks: JWKS missing keys field entirely" {
    try std.testing.expectError(VerifyError.JwksParseFailed, parseJwks(std.testing.allocator,
        \\{"other":"field"}
    ));
}

test "parseJwks: duplicate kids in JWKS (first match wins)" {
    const dup_key = "{\"kty\":\"RSA\",\"kid\":\"dup\",\"n\":\"" ++ fx.TEST_RSA_N ++ "\",\"e\":\"AQAB\"}";
    const jwks_dupes = "{\"keys\":[" ++ dup_key ++ "," ++ dup_key ++ "]}";
    var cache = try parseJwks(std.testing.allocator, jwks_dupes);
    defer cache.deinit(std.testing.allocator);
    try std.testing.expectEqual(@as(usize, 2), cache.keys.len);
    // Both keys stored — lookupKey returns first match
}

// ── RS256 signature verification edge cases ───────────────────────────

test "verifyRs256: wrong modulus length rejected" {
    const msg = "test.message";
    const bad_modulus = "short";
    const bad_sig = "short";
    const exp = fx.TEST_RSA_N; // any long base64url value serves as the exponent here
    try std.testing.expectError(VerifyError.SignatureInvalid, verifyRs256(msg, bad_sig, bad_modulus, exp));
}

test "verifyRs256: 128-byte modulus with wrong signature" {
    const msg = "header.payload";
    // 128-byte modulus (1024-bit key)
    var modulus: [128]u8 = undefined;
    @memset(&modulus, 0xff);
    var sig: [128]u8 = undefined;
    @memset(&sig, 0x00);
    const exp_bytes = [_]u8{ 0x01, 0x00, 0x01 }; // 65537
    try std.testing.expectError(VerifyError.SignatureInvalid, verifyRs256(msg, &sig, &modulus, &exp_bytes));
}

test "verifyRs256: empty signature" {
    try std.testing.expectError(VerifyError.SignatureInvalid, verifyRs256("msg", "", "x" ** 256, &[_]u8{ 1, 0, 1 }));
}

test "verifyRs256: signature length mismatch with modulus" {
    // 256-byte modulus but 128-byte signature
    var modulus: [256]u8 = undefined;
    @memset(&modulus, 0xff);
    var sig: [128]u8 = undefined;
    @memset(&sig, 0x00);
    try std.testing.expectError(VerifyError.SignatureInvalid, verifyRs256("msg", &sig, &modulus, &[_]u8{ 1, 0, 1 }));
}

// ── Bearer token injection vectors ────────────────────────────────────

test "extractBearerToken: CRLF injection attempt" {
    const t = try extractBearerToken("Bearer token\r\nX-Injected: evil");
    // The token includes the injected header — consumers must not use this in HTTP headers
    // Our code only passes it to JWT split/decode, which will reject it
    try std.testing.expect(t.len > 0);
}

test "extractBearerToken: tab padding is trimmed" {
    const t = try extractBearerToken("Bearer \ttoken123\t");
    try std.testing.expectEqualStrings("token123", t);
}

test "splitJwt: segments with whitespace" {
    // Whitespace in base64 segments should cause base64 decode failure downstream
    const parts = try splitJwt("aaa .bbb.ccc");
    try std.testing.expectEqualStrings("aaa ", parts.header_b64);
}

// ── verifyAndDecode: header-level attacks ──────────────────────────────

test "verifyAndDecode: header without kid field" {
    // {"alg":"RS256","typ":"JWT"} — no kid
    const header_no_kid = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9";
    const payload = "eyJzdWIiOiJ1c2VyIiwiaXNzIjoiaHR0cHM6Ly9jbGVyay5kZXYuYWdlbnRzZmxlZXQubmV0IiwiZXhwIjo0MTAyNDQ0ODAwfQ";
    var v = try makeTestVerifier(null);
    defer v.deinit();
    try std.testing.expectError(VerifyError.MissingKeyId, v.verifyAndDecode(
        std.testing.allocator,
        "Bearer " ++ header_no_kid ++ "." ++ payload ++ ".fakesig",
    ));
}

test "verifyAndDecode: header is not valid JSON" {
    // "not json" base64url encoded
    const bad_header = "bm90IGpzb24";
    var v = try makeTestVerifier(null);
    defer v.deinit();
    const result = v.verifyAndDecode(std.testing.allocator, "Bearer " ++ bad_header ++ ".cGF5bG9hZA.c2ln");
    try std.testing.expect(std.meta.isError(result));
}

test "verifyAndDecode: completely empty bearer value" {
    var v = try makeTestVerifier(null);
    defer v.deinit();
    try std.testing.expectError(VerifyError.InvalidAuthorization, v.verifyAndDecode(std.testing.allocator, "Bearer  "));
}

test "parseStandardClaims survives allocation failure without leaking" {
    // The subject/issuer dupes carry an errdefer ladder: a failed issuer dupe
    // frees the subject instead of leaking it inside the return literal.
    const Probe = struct {
        fn run(alloc: std.mem.Allocator) !void {
            const raw = try alloc.dupe(u8, "{\"sub\":\"u\",\"iss\":\"i\",\"exp\":99999999999}");
            var caller_owns_raw = true;
            defer if (caller_owns_raw) alloc.free(raw);
            const v = try parseStandardClaims(alloc, raw, null, null);
            caller_owns_raw = false; // transferred into v.claims_json
            alloc.free(v.subject);
            alloc.free(v.issuer);
            alloc.free(v.claims_json);
        }
    };
    try std.testing.checkAllAllocationFailures(std.testing.allocator, Probe.run, .{});
}

// A JWKS endpoint that dies mid-body: valid head promising 4096 bytes, 128
// sent, then close. The fetch fails and the partially-written body must be
// freed — before the deinit pairing fix this exact path leaked every retry.
const PartialJwksServer = struct {
    fn run(listener: *std.Io.net.Server, io: std.Io) void {
        const conn = listener.accept(io) catch return;
        defer conn.close(io);
        var buf: [2048]u8 = undefined;
        _ = std.posix.read(conn.socket.handle, &buf) catch return;
        // Truncated CHUNKED framing: one full chunk lands in the accumulator,
        // the terminal chunk never arrives — a guaranteed hard read error
        // (std's fetch tolerates a short content-length body).
        const head: []const u8 = "HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n80\r\n" ++ ("x" ** 128) ++ "\r\n";
        var sent: usize = 0;
        while (sent < head.len) {
            const rc = std.posix.system.write(conn.socket.handle, head[sent..].ptr, head.len - sent);
            if (std.posix.errno(rc) != .SUCCESS) return;
            sent += @intCast(rc);
        }
    }
};

fn partialServerPort(handle: std.Io.net.Socket.Handle) !u16 {
    // SAFETY: getsockname fills sa before sa.port is read on success.
    var sa: std.posix.sockaddr.in = undefined;
    var len: std.posix.socklen_t = @sizeOf(std.posix.sockaddr.in);
    if (std.c.getsockname(handle, @ptrCast(&sa), &len) != 0) return error.GetSockNameFailed;
    return std.mem.bigToNative(u16, sa.port);
}

test "jwks fetch frees the partial body when the endpoint dies mid-stream" {
    const io = common.globalIo();
    var addr = std.Io.net.IpAddress.parseIp4("127.0.0.1", 0) catch return error.SkipZigTest;
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = partialServerPort(listener.socket.handle) catch return error.SkipZigTest;
    const server = std.Thread.spawn(.{}, PartialJwksServer.run, .{ &listener, io }) catch return error.SkipZigTest;

    var url_buf: [48]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}/jwks.json", .{port});
    var v = try Verifier.init(std.testing.allocator, .{ .jwks_url = url });
    defer v.deinit();
    const r = v.checkJwksConnectivity();
    server.join();
    // testing.allocator's leak detector is the real assertion here.
    try std.testing.expectError(VerifyError.JwksFetchFailed, r);
}

// A JWKS endpoint that streams far past the named cap: the client must
// reject at JWKS_MAX_RESPONSE_BYTES instead of accumulating without bound.
const OverCapJwksServer = struct {
    const TOTAL_BYTES: usize = 300 * 1024; // past the 256 KiB cap

    fn run(listener: *std.Io.net.Server, io: std.Io) void {
        const conn = listener.accept(io) catch return;
        defer conn.close(io);
        var buf: [2048]u8 = undefined;
        _ = std.posix.read(conn.socket.handle, &buf) catch return;
        const head: []const u8 = "HTTP/1.1 200 OK\r\ncontent-length: 307200\r\n\r\n";
        writeAllFd(conn.socket.handle, head) catch return;
        const filler = [_]u8{'x'} ** 4096;
        var sent: usize = 0;
        while (sent < TOTAL_BYTES) : (sent += filler.len) {
            // The client hangs up at the cap; the resulting write error ends us.
            writeAllFd(conn.socket.handle, &filler) catch return;
        }
    }

    fn writeAllFd(fd: std.posix.fd_t, bytes: []const u8) !void {
        var sent: usize = 0;
        while (sent < bytes.len) {
            const rc = std.posix.system.write(fd, bytes[sent..].ptr, bytes.len - sent);
            if (std.posix.errno(rc) != .SUCCESS) return error.WriteFailed;
            sent += @intCast(rc);
        }
    }
};

test "jwks fetch rejects a response larger than the named cap" {
    const io = common.globalIo();
    var addr = std.Io.net.IpAddress.parseIp4("127.0.0.1", 0) catch return error.SkipZigTest;
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = partialServerPort(listener.socket.handle) catch return error.SkipZigTest;
    const server = std.Thread.spawn(.{}, OverCapJwksServer.run, .{ &listener, io }) catch return error.SkipZigTest;

    var url_buf: [48]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}/jwks.json", .{port});
    var v = try Verifier.init(std.testing.allocator, .{ .jwks_url = url });
    defer v.deinit();
    const r = v.checkJwksConnectivity();
    server.join();
    // The cap rejection surfaces as a fetch failure; the accumulated prefix
    // is freed (testing.allocator's leak detector is the second assertion).
    try std.testing.expectError(VerifyError.JwksFetchFailed, r);
}

test "verifier init survives allocation failure without leaking (no panic)" {
    // Boot-path OOM is an error the caller reports, never a process abort:
    // the sweep proves both the error return and the errdefer ladder across
    // the four config dupes.
    const Probe = struct {
        fn run(alloc: std.mem.Allocator) !void {
            var v = try Verifier.init(alloc, .{
                .jwks_url = "https://idp.example/jwks.json",
                .issuer = "https://idp.example",
                .audience = "https://api.example",
                .inline_jwks_json = "{\"keys\":[]}",
            });
            v.deinit();
        }
    };
    try std.testing.checkAllAllocationFailures(std.testing.allocator, Probe.run, .{});
}

// One-shot server delivering the canonical key set with correct framing — the
// success-path twin of PartialJwksServer (review find: the rewritten capped
// reader had only failure-path coverage).
const OkJwksServer = struct {
    fn run(listener: *std.Io.net.Server, io: std.Io) void {
        const conn = listener.accept(io) catch return;
        defer conn.close(io);
        var buf: [2048]u8 = undefined;
        _ = std.posix.read(conn.socket.handle, &buf) catch return;
        const resp: []const u8 = std.fmt.comptimePrint(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {d}\r\nconnection: close\r\n\r\n",
            .{TEST_JWKS.len},
        ) ++ TEST_JWKS;
        var sent: usize = 0;
        while (sent < resp.len) {
            const rc = std.posix.system.write(conn.socket.handle, resp[sent..].ptr, resp.len - sent);
            if (std.posix.errno(rc) != .SUCCESS) return;
            sent += @intCast(rc);
        }
    }
};

test "jwks fetch success path delivers the key set byte-intact over loopback" {
    const io = common.globalIo();
    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = partialServerPort(listener.socket.handle) catch return error.SkipZigTest;
    const server = std.Thread.spawn(.{}, OkJwksServer.run, .{ &listener, io }) catch return error.SkipZigTest;

    var url_buf: [64]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}/jwks.json", .{port});
    var v = try Verifier.init(std.testing.allocator, .{
        .jwks_url = url,
        .issuer = "https://clerk.dev.agentsfleet.net",
        .audience = "https://api.agentsfleet.net",
    });
    defer v.deinit();
    try v.checkJwksConnectivity();
    server.join();
    // A REAL token verifies against the FETCHED (not inline) key set — the
    // capped chunk reader delivered the body byte-intact, not merely un-huge.
    const vc = try v.verifyAndDecode(std.testing.allocator, "Bearer " ++ TEST_VALID_TOKEN);
    defer freeClaims(vc);
    try std.testing.expectEqualStrings("user_test", vc.subject);
}

// ── Compressed transport ───────────────────────────────────────────────
//
// Every test above either injects `inline_jwks_json` or serves an
// uncompressed body, so none of them ever exercised a negotiated
// content-encoding. The client advertises `accept-encoding: gzip, deflate`
// by default and real providers honour it — which is how a raw-reader body
// read shipped: the fetch handed gzip bytes to a JSON parser and took every
// token verification down with it. These tests fetch over real HTTP with
// real compression so that path cannot regress silently again.

const jwks_fetch = @import("jwks_fetch.zig");

const LOOPBACK_HOST = "127.0.0.1";
const LOOPBACK_URL_FMT = "http://127.0.0.1:{d}/jwks.json";
const URL_BUFFER_LEN = 64;
const REQUEST_SCRATCH_LEN = 2048;
const GZIP_SCRATCH_LEN = 4096;
const HEAD_SCRATCH_LEN = 256;
const HEAD_GZIP_FMT =
    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n" ++
    "content-encoding: gzip\r\ncontent-length: {d}\r\nconnection: close\r\n\r\n";
/// Inflates past `JWKS_MAX_RESPONSE_BYTES` from a few hundred wire bytes —
/// the decompression bomb in miniature. A wire-byte cap never sees it.
const BOMB_INFLATED_LEN: usize = 300 * 1024;
const BOMB_FILL_BYTE: u8 = 'A';

/// Gzip `raw` into `out_buf` and return the encoded slice. Compressed
/// fixtures are produced from the same bytes the assertions read, so no
/// opaque binary blob enters the repo and the expected plaintext is visible
/// at the call site.
fn gzipInto(alloc: std.mem.Allocator, out_buf: []u8, raw: []const u8) ![]u8 {
    const window = try alloc.alloc(u8, std.compress.flate.max_window_len);
    defer alloc.free(window);
    var out: std.Io.Writer = .fixed(out_buf);
    var c = try std.compress.flate.Compress.init(&out, window, .gzip, .default);
    try c.writer.writeAll(raw);
    try c.finish();
    return out.buffered();
}

/// Serves one canned head + body over loopback, then closes. `body` may be
/// short of what the head promises — that is the mid-stream death case.
const CannedJwksServer = struct {
    fn run(listener: *std.Io.net.Server, io: std.Io, head: []const u8, body: []const u8) void {
        const conn = listener.accept(io) catch return;
        defer conn.close(io);
        var buf: [REQUEST_SCRATCH_LEN]u8 = undefined;
        _ = std.posix.read(conn.socket.handle, &buf) catch return;
        OverCapJwksServer.writeAllFd(conn.socket.handle, head) catch return;
        OverCapJwksServer.writeAllFd(conn.socket.handle, body) catch return;
    }
};

test "jwks fetch decodes a gzip-encoded key set and verifies a real token" {
    const alloc = std.testing.allocator;
    const io = common.globalIo();

    var gzip_buf: [GZIP_SCRATCH_LEN]u8 = undefined;
    const body = try gzipInto(alloc, &gzip_buf, TEST_JWKS);
    // The fixture must actually be compressed, or the test proves nothing.
    try std.testing.expect(body.len > 2 and body[0] == 0x1f and body[1] == 0x8b);
    var head_buf: [HEAD_SCRATCH_LEN]u8 = undefined;
    const head = try std.fmt.bufPrint(&head_buf, HEAD_GZIP_FMT, .{body.len});

    var addr = try std.Io.net.IpAddress.parseIp4(LOOPBACK_HOST, 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = partialServerPort(listener.socket.handle) catch return error.SkipZigTest;
    const server = std.Thread.spawn(.{}, CannedJwksServer.run, .{ &listener, io, head, body }) catch
        return error.SkipZigTest;

    var url_buf: [URL_BUFFER_LEN]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, LOOPBACK_URL_FMT, .{port});
    var v = try Verifier.init(alloc, .{
        .jwks_url = url,
        .issuer = "https://clerk.dev.agentsfleet.net",
        .audience = "https://api.agentsfleet.net",
    });
    defer v.deinit();
    try v.checkJwksConnectivity();
    server.join();

    // The whole point: a real token verifies against a key set that arrived
    // compressed. Before the decoding read this raised JwksParseFailed and
    // every authenticated route answered 503.
    const vc = try v.verifyAndDecode(alloc, "Bearer " ++ TEST_VALID_TOKEN);
    defer freeClaims(vc);
    try std.testing.expectEqualStrings("user_test", vc.subject);
}

test "jwks fetch rejects a compressed body that inflates past the cap" {
    const alloc = std.testing.allocator;
    const io = common.globalIo();

    const inflated = try alloc.alloc(u8, BOMB_INFLATED_LEN);
    defer alloc.free(inflated);
    @memset(inflated, BOMB_FILL_BYTE);
    var gzip_buf: [GZIP_SCRATCH_LEN]u8 = undefined;
    const body = try gzipInto(alloc, &gzip_buf, inflated);
    // A wire-byte cap would wave this through: it is orders of magnitude
    // under JWKS_MAX_RESPONSE_BYTES on the wire and far over it decoded.
    try std.testing.expect(body.len < jwks_fetch.JWKS_MAX_RESPONSE_BYTES);
    var head_buf: [HEAD_SCRATCH_LEN]u8 = undefined;
    const head = try std.fmt.bufPrint(&head_buf, HEAD_GZIP_FMT, .{body.len});

    var addr = try std.Io.net.IpAddress.parseIp4(LOOPBACK_HOST, 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = partialServerPort(listener.socket.handle) catch return error.SkipZigTest;
    const server = std.Thread.spawn(.{}, CannedJwksServer.run, .{ &listener, io, head, body }) catch
        return error.SkipZigTest;

    var url_buf: [URL_BUFFER_LEN]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, LOOPBACK_URL_FMT, .{port});
    // Asserted through fetchCapped, not the Verifier: the Verifier collapses
    // every transport outcome into JwksFetchFailed, which cannot tell a cap
    // refusal from an unreachable provider (Invariant 2).
    const r = jwks_fetch.fetchCapped(alloc, url);
    server.join();
    try std.testing.expectError(jwks_fetch.FetchError.ResponseTooLarge, r);
}

test "jwks fetch keeps the cap refusal distinct from a transport fault" {
    const alloc = std.testing.allocator;
    const io = common.globalIo();

    // Declares gzip, sends plain text: the decoder rejects it. That is a
    // transport fault, NOT a size refusal, and never a silent empty key set.
    const body = TEST_JWKS;
    var head_buf: [HEAD_SCRATCH_LEN]u8 = undefined;
    const head = try std.fmt.bufPrint(&head_buf, HEAD_GZIP_FMT, .{body.len});

    var addr = try std.Io.net.IpAddress.parseIp4(LOOPBACK_HOST, 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = partialServerPort(listener.socket.handle) catch return error.SkipZigTest;
    const server = std.Thread.spawn(.{}, CannedJwksServer.run, .{ &listener, io, head, body }) catch
        return error.SkipZigTest;

    var url_buf: [URL_BUFFER_LEN]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, LOOPBACK_URL_FMT, .{port});
    const r = jwks_fetch.fetchCapped(alloc, url);
    server.join();
    try std.testing.expectError(jwks_fetch.FetchError.FetchFailed, r);
}

test "jwks fetch frees the partial body when a compressed stream dies mid-body" {
    const alloc = std.testing.allocator;
    const io = common.globalIo();

    var gzip_buf: [GZIP_SCRATCH_LEN]u8 = undefined;
    const full = try gzipInto(alloc, &gzip_buf, TEST_JWKS);
    var head_buf: [HEAD_SCRATCH_LEN]u8 = undefined;
    // Head promises the whole body; the server sends half and hangs up.
    const head = try std.fmt.bufPrint(&head_buf, HEAD_GZIP_FMT, .{full.len});
    const truncated = full[0 .. full.len / 2];

    var addr = try std.Io.net.IpAddress.parseIp4(LOOPBACK_HOST, 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = partialServerPort(listener.socket.handle) catch return error.SkipZigTest;
    const server = std.Thread.spawn(.{}, CannedJwksServer.run, .{ &listener, io, head, truncated }) catch
        return error.SkipZigTest;

    var url_buf: [URL_BUFFER_LEN]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, LOOPBACK_URL_FMT, .{port});
    const r = jwks_fetch.fetchCapped(alloc, url);
    server.join();
    // The decompression buffer and the partial accumulation both have exactly
    // one owner; testing.allocator's leak detector is the second assertion.
    try std.testing.expectError(jwks_fetch.FetchError.FetchFailed, r);
}

test "jwks fetch still refuses an oversize uncompressed body as a cap refusal" {
    const alloc = std.testing.allocator;
    const io = common.globalIo();
    var addr = try std.Io.net.IpAddress.parseIp4(LOOPBACK_HOST, 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = partialServerPort(listener.socket.handle) catch return error.SkipZigTest;
    const server = std.Thread.spawn(.{}, OverCapJwksServer.run, .{ &listener, io }) catch
        return error.SkipZigTest;

    var url_buf: [URL_BUFFER_LEN]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, LOOPBACK_URL_FMT, .{port});
    // M152's guarantee, unbroken by the move to decompressed accounting: the
    // identity path still refuses at the cap, and still as ResponseTooLarge.
    const r = jwks_fetch.fetchCapped(alloc, url);
    server.join();
    try std.testing.expectError(jwks_fetch.FetchError.ResponseTooLarge, r);
}

test "jwks fetch accepts a decoded body of exactly the cap" {
    const alloc = std.testing.allocator;
    const io = common.globalIo();

    // The boundary the cap's comparison turns on: `> CAP` accepts exactly CAP,
    // `>= CAP` would refuse it. Nothing else in the suite pins which one ships.
    const at_cap = try alloc.alloc(u8, jwks_fetch.JWKS_MAX_RESPONSE_BYTES);
    defer alloc.free(at_cap);
    @memset(at_cap, BOMB_FILL_BYTE);
    var gzip_buf: [GZIP_SCRATCH_LEN]u8 = undefined;
    const body = try gzipInto(alloc, &gzip_buf, at_cap);
    var head_buf: [HEAD_SCRATCH_LEN]u8 = undefined;
    const head = try std.fmt.bufPrint(&head_buf, HEAD_GZIP_FMT, .{body.len});

    var addr = try std.Io.net.IpAddress.parseIp4(LOOPBACK_HOST, 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = partialServerPort(listener.socket.handle) catch return error.SkipZigTest;
    const server = std.Thread.spawn(.{}, CannedJwksServer.run, .{ &listener, io, head, body }) catch
        return error.SkipZigTest;

    var url_buf: [URL_BUFFER_LEN]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, LOOPBACK_URL_FMT, .{port});
    const fetched = jwks_fetch.fetchCapped(alloc, url);
    server.join();
    const bytes = try fetched;
    defer alloc.free(bytes);
    try std.testing.expectEqual(jwks_fetch.JWKS_MAX_RESPONSE_BYTES, bytes.len);
}

test "jwks fetch refuses an encoding it never advertised" {
    const alloc = std.testing.allocator;
    const io = common.globalIo();

    // The client advertises gzip/deflate/identity. A provider answering zstd
    // is misbehaving; the fetch must refuse rather than mis-decode or size a
    // window for it. Refused at receiveHead, surfacing as a transport fault.
    const body = TEST_JWKS;
    var head_buf: [HEAD_SCRATCH_LEN]u8 = undefined;
    const head = try std.fmt.bufPrint(
        &head_buf,
        "HTTP/1.1 200 OK\r\ncontent-encoding: zstd\r\ncontent-length: {d}\r\nconnection: close\r\n\r\n",
        .{body.len},
    );

    var addr = try std.Io.net.IpAddress.parseIp4(LOOPBACK_HOST, 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = partialServerPort(listener.socket.handle) catch return error.SkipZigTest;
    const server = std.Thread.spawn(.{}, CannedJwksServer.run, .{ &listener, io, head, body }) catch
        return error.SkipZigTest;

    var url_buf: [URL_BUFFER_LEN]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, LOOPBACK_URL_FMT, .{port});
    const r = jwks_fetch.fetchCapped(alloc, url);
    server.join();
    try std.testing.expectError(jwks_fetch.FetchError.FetchFailed, r);
}

test "jwks fetch treats a non-200 key-set response as a transport fault" {
    const alloc = std.testing.allocator;
    const io = common.globalIo();

    // A rotated or mistyped issuer leaves the well-known path answering 404.
    // That must read as "provider unreachable", not as an empty key set —
    // an empty set would fail every token with a kid miss instead.
    const head = "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";

    var addr = try std.Io.net.IpAddress.parseIp4(LOOPBACK_HOST, 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = partialServerPort(listener.socket.handle) catch return error.SkipZigTest;
    const server = std.Thread.spawn(.{}, CannedJwksServer.run, .{ &listener, io, head, "" }) catch
        return error.SkipZigTest;

    var url_buf: [URL_BUFFER_LEN]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, LOOPBACK_URL_FMT, .{port});
    const r = jwks_fetch.fetchCapped(alloc, url);
    server.join();
    try std.testing.expectError(jwks_fetch.FetchError.FetchFailed, r);
}

test "jwks fetch rejects an unparseable issuer url without touching the network" {
    // A malformed OIDC_ISSUER must fail closed at parse time rather than
    // reaching the socket layer. No listener is started: reaching the network
    // would hang this test rather than pass it.
    const r = jwks_fetch.fetchCapped(std.testing.allocator, "not-a-url");
    try std.testing.expectError(jwks_fetch.FetchError.FetchFailed, r);
}
