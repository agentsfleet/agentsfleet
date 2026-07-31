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

const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","kid":"test-kid-static","use":"sig","alg":"RS256","n":"7ZUw6J4OYDXLJPGWADVw2-IgBawVd55H1Xh4R_FFFFYVNdG2O7EcTvBlFZhRzxDW9uL-SvxCt6slRDXDlZo9fmSI9yki7z8RAJZokcekxdP8za5w7g4QAoFeSieDhWWChkzHJ-vDGkrr0SAn8n4lIwpya-vCbO1eXmmz4Ay0pjenWyyGB1j371Zk2JGkAEJB347oJcVDMqVDt3d-TR0fyyspVw0nNxdDkZgNuB0EXOuEV4WvWgj0dtzwURhTI82AfpgheV23Kz7np9EoPxAhkfuslAjpRfqlRCXOOfmik-T6nvCe-fFPmHRwIY_zc1VrtwjKF0TjeALm4CCj_0pjRQ","e":"AQAB"}]}
;
const TEST_HEADER = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9";
const TEST_PAYLOAD_VALID = "eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJpYXQiOjE3MDQwNjcyMDAsIm9yZ19pZCI6Im9yZ18xIiwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoidGVuYW50X2EifSwiZXhwIjo0MTAyNDQ0ODAwfQ";
const TEST_PAYLOAD_EXPIRED = "eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJpYXQiOjE3MDQwNjcyMDAsIm9yZ19pZCI6Im9yZ18xIiwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoidGVuYW50X2EifSwiZXhwIjoxNzA0MDY3MzAwfQ";
const TEST_SIG_VALID = "pU5Y3T5yhLjleABex4K0fsyfjrxHDFa-8sjbI5hQhPHVw7P-WF_72VbWoCa9sVPi5cwGU0tbj8rZY2BMhq36_xZxwh7l4Z9SdguVGCiceDuqhhtRxA8vdPIlolrrykxAuEvlyeHRiE1uOzSvSGZZFCHvkgVK06SwC4oK1NlSgFx_cjKYbY0NychCG0XxLrl5XUoR79va4-9HGRMDYaTFRMutwMzFF_4iCbpn3RHl-qu9_RAabJrsQkeCmYYXaQKLt_aVVfrBMQWOwJDvCuTaeJcRGJefKmNdc-aM8mqBjZX9RIocD_hp5ADxY9HZdBFtGz7OAofgM2ZqVeJPkvNKfQ";
const TEST_SIG_EXPIRED = "Ctiud6VRvF54eited-UOq6HEiKZWNdhPli_w_rsFLmS6bmeDr2xuXlag6HgZLCnOc1mHsoJGGqeojZ8xt2SVt6JHjxXxV6KhP6orw4FPgmPKzyZw2zFWGmi3M0IUSv9CzsaaWnoj5KdLL9DWzF--EpMDddqaEMBLolxuMV_uO0m6Fji6lJikVZaPTFJ0YMzcMdkvh4iZ9_y2fGYvjUSPnbNw-3eq4P4tlUq2n_6ES17zLGIF55cRoUo7v-audITTd9AVwp0Y3-_PXq-yAJEPTBhysG1iYiKMrf9x_r1h6g2rKCFQ_7k48K-o_rUPSFVaU0Q3TXG3CDoMjmAma0LN6A";
const TEST_VALID_TOKEN = TEST_HEADER ++ "." ++ TEST_PAYLOAD_VALID ++ "." ++ TEST_SIG_VALID;
const TEST_EXPIRED_TOKEN = TEST_HEADER ++ "." ++ TEST_PAYLOAD_EXPIRED ++ "." ++ TEST_SIG_EXPIRED;
// Same RSA key as TEST_JWKS but published under a different kid — models the
// pre-rotation key set that does NOT contain the test token's kid.
const WRONG_KID_JWKS =
    \\{"keys":[{"kty":"RSA","kid":"wrong-kid","use":"sig","alg":"RS256","n":"7ZUw6J4OYDXLJPGWADVw2-IgBawVd55H1Xh4R_FFFFYVNdG2O7EcTvBlFZhRzxDW9uL-SvxCt6slRDXDlZo9fmSI9yki7z8RAJZokcekxdP8za5w7g4QAoFeSieDhWWChkzHJ-vDGkrr0SAn8n4lIwpya-vCbO1eXmmz4Ay0pjenWyyGB1j371Zk2JGkAEJB347oJcVDMqVDt3d-TR0fyyspVw0nNxdDkZgNuB0EXOuEV4WvWgj0dtzwURhTI82AfpgheV23Kz7np9EoPxAhkfuslAjpRfqlRCXOOfmik-T6nvCe-fFPmHRwIY_zc1VrtwjKF0TjeALm4CCj_0pjRQ","e":"AQAB"}]}
;

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
    try std.testing.expectError(VerifyError.JwksParseFailed, parseJwks(std.testing.allocator,
        \\{"keys":[{"kty":"RSA","n":"7ZUw6J4OYDXLJPGWADVw2-IgBawVd55H1Xh4R_FFFFYVNdG2O7EcTvBlFZhRzxDW9uL-SvxCt6slRDXDlZo9fmSI9yki7z8RAJZokcekxdP8za5w7g4QAoFeSieDhWWChkzHJ-vDGkrr0SAn8n4lIwpya-vCbO1eXmmz4Ay0pjenWyyGB1j371Zk2JGkAEJB347oJcVDMqVDt3d-TR0fyyspVw0nNxdDkZgNuB0EXOuEV4WvWgj0dtzwURhTI82AfpgheV23Kz7np9EoPxAhkfuslAjpRfqlRCXOOfmik-T6nvCe-fFPmHRwIY_zc1VrtwjKF0TjeALm4CCj_0pjRQ","e":"AQAB"}]}
    ));
}

test "parseJwks: non-RSA key is skipped" {
    try std.testing.expectError(VerifyError.JwksParseFailed, parseJwks(std.testing.allocator,
        \\{"keys":[{"kty":"EC","kid":"ec1","n":"7ZUw6J4OYDXLJPGWADVw2-IgBawVd55H1Xh4R_FFFFYVNdG2O7EcTvBlFZhRzxDW9uL-SvxCt6slRDXDlZo9fmSI9yki7z8RAJZokcekxdP8za5w7g4QAoFeSieDhWWChkzHJ-vDGkrr0SAn8n4lIwpya-vCbO1eXmmz4Ay0pjenWyyGB1j371Zk2JGkAEJB347oJcVDMqVDt3d-TR0fyyspVw0nNxdDkZgNuB0EXOuEV4WvWgj0dtzwURhTI82AfpgheV23Kz7np9EoPxAhkfuslAjpRfqlRCXOOfmik-T6nvCe-fFPmHRwIY_zc1VrtwjKF0TjeALm4CCj_0pjRQ","e":"AQAB"}]}
    ));
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
    const tampered = TEST_HEADER ++ "." ++ "eXJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJpYXQiOjE3MDQwNjcyMDAsIm9yZ19pZCI6Im9yZ18xIiwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoidGVuYW50X2EifSwiZXhwIjo0MTAyNDQ0ODAwfQ" ++ "." ++ TEST_SIG_VALID;
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
    const jwks_dupes =
        \\{"keys":[{"kty":"RSA","kid":"dup","n":"7ZUw6J4OYDXLJPGWADVw2-IgBawVd55H1Xh4R_FFFFYVNdG2O7EcTvBlFZhRzxDW9uL-SvxCt6slRDXDlZo9fmSI9yki7z8RAJZokcekxdP8za5w7g4QAoFeSieDhWWChkzHJ-vDGkrr0SAn8n4lIwpya-vCbO1eXmmz4Ay0pjenWyyGB1j371Zk2JGkAEJB347oJcVDMqVDt3d-TR0fyyspVw0nNxdDkZgNuB0EXOuEV4WvWgj0dtzwURhTI82AfpgheV23Kz7np9EoPxAhkfuslAjpRfqlRCXOOfmik-T6nvCe-fFPmHRwIY_zc1VrtwjKF0TjeALm4CCj_0pjRQ","e":"AQAB"},{"kty":"RSA","kid":"dup","n":"7ZUw6J4OYDXLJPGWADVw2-IgBawVd55H1Xh4R_FFFFYVNdG2O7EcTvBlFZhRzxDW9uL-SvxCt6slRDXDlZo9fmSI9yki7z8RAJZokcekxdP8za5w7g4QAoFeSieDhWWChkzHJ-vDGkrr0SAn8n4lIwpya-vCbO1eXmmz4Ay0pjenWyyGB1j371Zk2JGkAEJB347oJcVDMqVDt3d-TR0fyyspVw0nNxdDkZgNuB0EXOuEV4WvWgj0dtzwURhTI82AfpgheV23Kz7np9EoPxAhkfuslAjpRfqlRCXOOfmik-T6nvCe-fFPmHRwIY_zc1VrtwjKF0TjeALm4CCj_0pjRQ","e":"AQAB"}]}
    ;
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
    const exp = "7ZUw6J4OYDXLJPGWADVw2-IgBawVd55H1Xh4R_FFFFYVNdG2O7EcTvBlFZhRzxDW9uL-SvxCt6slRDXDlZo9fmSI9yki7z8RAJZokcekxdP8za5w7g4QAoFeSieDhWWChkzHJ-vDGkrr0SAn8n4lIwpya-vCbO1eXmmz4Ay0pjenWyyGB1j371Zk2JGkAEJB347oJcVDMqVDt3d-TR0fyyspVw0nNxdDkZgNuB0EXOuEV4WvWgj0dtzwURhTI82AfpgheV23Kz7np9EoPxAhkfuslAjpRfqlRCXOOfmik-T6nvCe-fFPmHRwIY_zc1VrtwjKF0TjeALm4CCj_0pjRQ";
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
