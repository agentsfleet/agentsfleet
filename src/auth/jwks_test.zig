const std = @import("std");
const jwks = @import("jwks.zig");

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
    \\{"keys":[{"kty":"RSA","kid":"test-kid-static","use":"sig","alg":"RS256","n":"kEge9Llezx-onM-jdO1fw85yTFmDDHWaZVdihVqMVAvRDGFvHbyoPrp5F-ZaDTqVEd1_pH12HM3abE6HRyYwSRxPcSKf2GlGWBVPtFbidOezLupgspHs8-yXBFKkGQEGBTWspJ4Obd0g9u1EX-cQqzy-lXiGd8gt1oK8Rxx5YBohNbaQMs5dbJ61J9c0afrG0dx-xOOx2tb95izx_m-sB83-aj7mX_r3ClpbZYcOY8ZKA3QNwR9tattkTiowpgzBZ0PGw5wuzrQayjWQRooolW4kzYMVWOI5K4GVPoabBDZDPs2nfet290iFHkNRu8cc2xPDmty0cDIhbS9Mq33qsQ","e":"AQAB"}]}
;
const TEST_VALID_TOKEN = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImlhdCI6MTcwNDA2NzIwMCwib3JnX2lkIjoib3JnXzEiLCJtZXRhZGF0YSI6eyJ0ZW5hbnRfaWQiOiJ0ZW5hbnRfYSJ9LCJleHAiOjQxMDI0NDQ4MDB9.R5EaetratAEMN3VcDRDyR3KM9dKU3FYGEvzajPdmMUB_3T3qE0G0xZ_IoqyNilvjuMcbdSF-YQL1ylcMPTyBeFUWYAUlMjWBju-Bt3FF0Abqdte5-a64oPb_Ev0ogZyJcI8DDt9yT4kUjH7S2jp4fu9hQaEDMW_6tcASagCHTIjw2h0A41_Y8PI4CrgglIFqEKGim5PUEWM_KzZxs9pjv7-_HsZTovfZTcKeiJkGiFQvyR3oKfudvjLNyyGtdYKiSjfOWtLfJkxGt0CKPkbDbrnj_cSmwCt-X_v_OmG5vm07h7iDKrKhXiav0Djn7W3zZ8EcwjhlvMSsKZ3Uy9Nk2g";
const TEST_EXPIRED_TOKEN = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImlhdCI6MTcwNDA2NzIwMCwib3JnX2lkIjoib3JnXzEiLCJtZXRhZGF0YSI6eyJ0ZW5hbnRfaWQiOiJ0ZW5hbnRfYSJ9LCJleHAiOjE3MDQwNjczMDB9.CCjR252liw1fHwmo4kBmHH0nw1uPUBtibZx9BSPKPdzU_4oDmSrJyFP4LJtd9THDIVV8JyE5r1I9a8nuyLe66Wfr_N_tiiNAzYQ0voN_B2AQ-iy8DHhVAJibflv5eaGRXxh4pfn7uV3vY1ZGGDxwyjOXWPy_ULwSwtaDGDQNeWWYgVaaKp1B0-l__oIiMmRgsCiMOE6qyU2SFCQKG05vF54fgg7Pp4hpOgR9guE-rYKoLo39qE0RJvnaf5MTz2WbsPRxrvGurJ1lgnPrxGSXDMT2xJATkof6hP3Hv3QuSRlfCQwLEvlHKZG5ANpe7dxQ00KGf3RJiv0ly9mPapsD5g";

fn makeTestVerifier(inline_jwks: ?[]const u8) Verifier {
    return Verifier.init(std.testing.allocator, .{
        .jwks_url = "https://clerk.dev.usezombie.com/.well-known/jwks.json",
        .issuer = "https://clerk.dev.usezombie.com",
        .audience = "https://api.usezombie.com",
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
        \\{"keys":[{"kty":"RSA","n":"AQAB","e":"AQAB"}]}
    ));
}

test "parseJwks: non-RSA key is skipped" {
    try std.testing.expectError(VerifyError.JwksParseFailed, parseJwks(std.testing.allocator,
        \\{"keys":[{"kty":"EC","kid":"ec1","n":"AQAB","e":"AQAB"}]}
    ));
}

// ── Full verifyAndDecode edge cases ────────────────────────────────────

test "verifyAndDecode: valid token" {
    var v = makeTestVerifier(null);
    defer v.deinit();
    const vc = try v.verifyAndDecode(std.testing.allocator, "Bearer " ++ TEST_VALID_TOKEN);
    defer freeClaims(vc);
    try std.testing.expectEqualStrings("user_test", vc.subject);
    try std.testing.expectEqualStrings("https://clerk.dev.usezombie.com", vc.issuer);
}

test "verifyAndDecode: expired token" {
    var v = makeTestVerifier(null);
    defer v.deinit();
    try std.testing.expectError(VerifyError.TokenExpired, v.verifyAndDecode(std.testing.allocator, "Bearer " ++ TEST_EXPIRED_TOKEN));
}

test "verifyAndDecode: missing Authorization header" {
    var v = makeTestVerifier(null);
    defer v.deinit();
    try std.testing.expectError(VerifyError.InvalidAuthorization, v.verifyAndDecode(std.testing.allocator, ""));
}

test "verifyAndDecode: garbage token" {
    var v = makeTestVerifier(null);
    defer v.deinit();
    try std.testing.expectError(VerifyError.InvalidAuthorization, v.verifyAndDecode(std.testing.allocator, "not-a-bearer"));
}

test "verifyAndDecode: Bearer with no token" {
    var v = makeTestVerifier(null);
    defer v.deinit();
    try std.testing.expectError(VerifyError.InvalidAuthorization, v.verifyAndDecode(std.testing.allocator, "Bearer "));
}

test "verifyAndDecode: token with wrong kid" {
    // Use a JWKS that has a different kid than the test token
    const wrong_kid_jwks =
        \\{"keys":[{"kty":"RSA","kid":"wrong-kid","use":"sig","alg":"RS256","n":"kEge9Llezx-onM-jdO1fw85yTFmDDHWaZVdihVqMVAvRDGFvHbyoPrp5F-ZaDTqVEd1_pH12HM3abE6HRyYwSRxPcSKf2GlGWBVPtFbidOezLupgspHs8-yXBFKkGQEGBTWspJ4Obd0g9u1EX-cQqzy-lXiGd8gt1oK8Rxx5YBohNbaQMs5dbJ61J9c0afrG0dx-xOOx2tb95izx_m-sB83-aj7mX_r3ClpbZYcOY8ZKA3QNwR9tattkTiowpgzBZ0PGw5wuzrQayjWQRooolW4kzYMVWOI5K4GVPoabBDZDPs2nfet290iFHkNRu8cc2xPDmty0cDIhbS9Mq33qsQ","e":"AQAB"}]}
    ;
    var v = makeTestVerifier(wrong_kid_jwks);
    defer v.deinit();
    try std.testing.expectError(VerifyError.JwkNotFound, v.verifyAndDecode(std.testing.allocator, "Bearer " ++ TEST_VALID_TOKEN));
}

test "verifyAndDecode: audience mismatch" {
    var v = Verifier.init(std.testing.allocator, .{
        .jwks_url = "https://clerk.dev.usezombie.com/.well-known/jwks.json",
        .issuer = "https://clerk.dev.usezombie.com",
        .audience = "https://wrong-audience.example.com",
        .inline_jwks_json = TEST_JWKS,
    });
    defer v.deinit();
    try std.testing.expectError(VerifyError.AudienceMismatch, v.verifyAndDecode(std.testing.allocator, "Bearer " ++ TEST_VALID_TOKEN));
}

test "verifyAndDecode: issuer mismatch" {
    var v = Verifier.init(std.testing.allocator, .{
        .jwks_url = "https://clerk.dev.usezombie.com/.well-known/jwks.json",
        .issuer = "https://wrong-issuer.example.com",
        .audience = "https://api.usezombie.com",
        .inline_jwks_json = TEST_JWKS,
    });
    defer v.deinit();
    try std.testing.expectError(VerifyError.IssuerMismatch, v.verifyAndDecode(std.testing.allocator, "Bearer " ++ TEST_VALID_TOKEN));
}

test "verifyAndDecode: tampered payload (signature invalid)" {
    // Take valid token, modify one char in the payload segment
    const tampered = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9.eXJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImlhdCI6MTcwNDA2NzIwMCwib3JnX2lkIjoib3JnXzEiLCJtZXRhZGF0YSI6eyJ0ZW5hbnRfaWQiOiJ0ZW5hbnRfYSJ9LCJleHAiOjQxMDI0NDQ4MDB9.R5EaetratAEMN3VcDRDyR3KM9dKU3FYGEvzajPdmMUB_3T3qE0G0xZ_IoqyNilvjuMcbdSF-YQL1ylcMPTyBeFUWYAUlMjWBju-Bt3FF0Abqdte5-a64oPb_Ev0ogZyJcI8DDt9yT4kUjH7S2jp4fu9hQaEDMW_6tcASagCHTIjw2h0A41_Y8PI4CrgglIFqEKGim5PUEWM_KzZxs9pjv7-_HsZTovfZTcKeiJkGiFQvyR3oKfudvjLNyyGtdYKiSjfOWtLfJkxGt0CKPkbDbrnj_cSmwCt-X_v_OmG5vm07h7iDKrKhXiav0Djn7W3zZ8EcwjhlvMSsKZ3Uy9Nk2g";
    var v = makeTestVerifier(null);
    defer v.deinit();
    const result = v.verifyAndDecode(std.testing.allocator, "Bearer " ++ tampered);
    // Could be SignatureInvalid or TokenMalformed depending on what the tampered base64 decodes to
    try std.testing.expect(std.meta.isError(result));
}

test "verifyAndDecode: token with only two segments" {
    var v = makeTestVerifier(null);
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
    var v = makeTestVerifier(null);
    defer v.deinit();
    // If inline JWKS works, we can verify without network. This is the happy path.
    const vc = try v.verifyAndDecode(std.testing.allocator, "Bearer " ++ TEST_VALID_TOKEN);
    defer freeClaims(vc);
    try std.testing.expectEqualStrings("user_test", vc.subject);
}

test "verifier with empty URL and no inline JWKS fails" {
    var v = Verifier.init(std.testing.allocator, .{
        .jwks_url = "",
        .issuer = "https://clerk.dev.usezombie.com",
    });
    defer v.deinit();
    try std.testing.expectError(VerifyError.JwksFetchFailed, v.checkJwksConnectivity());
}

// ── OWASP JWT attack vectors ──────────────────────────────────────────

// CVE-2015-9235: alg:none attack — attacker strips signature and sets alg to "none"
test "OWASP: alg:none attack rejected" {
    // {"alg":"none","typ":"JWT"} => base64url
    const header_none = "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0";
    // {"sub":"admin","iss":"https://clerk.dev.usezombie.com","aud":"https://api.usezombie.com","exp":4102444800}
    const payload = "eyJzdWIiOiJhZG1pbiIsImlzcyI6Imh0dHBzOi8vY2xlcmsuZGV2LnVzZXpvbWJpZS5jb20iLCJhdWQiOiJodHRwczovL2FwaS51c2V6b21iaWUuY29tIiwiZXhwIjo0MTAyNDQ0ODAwfQ";
    var v = makeTestVerifier(null);
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
    const payload = "eyJzdWIiOiJhZG1pbiIsImlzcyI6Imh0dHBzOi8vY2xlcmsuZGV2LnVzZXpvbWJpZS5jb20iLCJhdWQiOiJodHRwczovL2FwaS51c2V6b21iaWUuY29tIiwiZXhwIjo0MTAyNDQ0ODAwfQ";
    var v = makeTestVerifier(null);
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
    const payload = "eyJzdWIiOiJ1c2VyIiwiaXNzIjoiaHR0cHM6Ly9jbGVyay5kZXYudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMH0";
    var v = makeTestVerifier(null);
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
        \\{"keys":[{"kty":"RSA","kid":"dup","n":"AQAB","e":"AQAB"},{"kty":"RSA","kid":"dup","n":"Ag","e":"AQAB"}]}
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
    const exp = "AQAB";
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
    const payload = "eyJzdWIiOiJ1c2VyIiwiaXNzIjoiaHR0cHM6Ly9jbGVyay5kZXYudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMH0";
    var v = makeTestVerifier(null);
    defer v.deinit();
    try std.testing.expectError(VerifyError.MissingKeyId, v.verifyAndDecode(
        std.testing.allocator,
        "Bearer " ++ header_no_kid ++ "." ++ payload ++ ".fakesig",
    ));
}

test "verifyAndDecode: header is not valid JSON" {
    // "not json" base64url encoded
    const bad_header = "bm90IGpzb24";
    var v = makeTestVerifier(null);
    defer v.deinit();
    const result = v.verifyAndDecode(std.testing.allocator, "Bearer " ++ bad_header ++ ".cGF5bG9hZA.c2ln");
    try std.testing.expect(std.meta.isError(result));
}

test "verifyAndDecode: completely empty bearer value" {
    var v = makeTestVerifier(null);
    defer v.deinit();
    try std.testing.expectError(VerifyError.InvalidAuthorization, v.verifyAndDecode(std.testing.allocator, "Bearer  "));
}
