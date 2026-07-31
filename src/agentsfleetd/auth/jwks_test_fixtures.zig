//! Shared JWKS/JWT test fixtures — the single source for the RSA test key
//! and its pre-signed tokens. Consumers: `jwks_test.zig`, the `oidc.zig`
//! test block, and the `middleware/bearer_or_api_key.zig` test block.
//! Test-only material: the signing private key is not in the repo, so these
//! constants can verify but never mint.

/// Base64url modulus of the shared 2048-bit RSA test key ("test-kid-static").
pub const TEST_RSA_N = "7ZUw6J4OYDXLJPGWADVw2-IgBawVd55H1Xh4R_FFFFYVNdG2O7EcTvBlFZhRzxDW9uL-SvxCt6slRDXDlZo9fmSI9yki7z8RAJZokcekxdP8za5w7g4QAoFeSieDhWWChkzHJ-vDGkrr0SAn8n4lIwpya-vCbO1eXmmz4Ay0pjenWyyGB1j371Zk2JGkAEJB347oJcVDMqVDt3d-TR0fyyspVw0nNxdDkZgNuB0EXOuEV4WvWgj0dtzwURhTI82AfpgheV23Kz7np9EoPxAhkfuslAjpRfqlRCXOOfmik-T6nvCe-fFPmHRwIY_zc1VrtwjKF0TjeALm4CCj_0pjRQ";

pub const TEST_KID = "test-kid-static";

/// One-key RSA key set for `kid` over the shared modulus — the single
/// spelling of the JWKS envelope (RULE UFS).
fn rsaKeySet(comptime kid: []const u8) []const u8 {
    return "{\"keys\":[{\"kty\":\"RSA\",\"kid\":\"" ++ kid ++ "\",\"use\":\"sig\",\"alg\":\"RS256\",\"n\":\"" ++ TEST_RSA_N ++ "\",\"e\":\"AQAB\"}]}";
}

pub const TEST_JWKS = rsaKeySet(TEST_KID);

/// Same RSA key but published under a different kid — the pre-rotation key
/// set that does NOT contain the test token's kid.
pub const WRONG_KID_JWKS = rsaKeySet("wrong-kid");

// Pre-signed JWT pieces for the key above.
// header: {"alg":"RS256","typ":"JWT","kid":"test-kid-static"}
pub const TEST_HEADER = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9";
pub const TEST_PAYLOAD_VALID = "eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJpYXQiOjE3MDQwNjcyMDAsIm9yZ19pZCI6Im9yZ18xIiwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoidGVuYW50X2EifSwiZXhwIjo0MTAyNDQ0ODAwfQ";
pub const TEST_PAYLOAD_EXPIRED = "eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJpYXQiOjE3MDQwNjcyMDAsIm9yZ19pZCI6Im9yZ18xIiwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoidGVuYW50X2EifSwiZXhwIjoxNzA0MDY3MzAwfQ";
pub const TEST_SIG_VALID = "pU5Y3T5yhLjleABex4K0fsyfjrxHDFa-8sjbI5hQhPHVw7P-WF_72VbWoCa9sVPi5cwGU0tbj8rZY2BMhq36_xZxwh7l4Z9SdguVGCiceDuqhhtRxA8vdPIlolrrykxAuEvlyeHRiE1uOzSvSGZZFCHvkgVK06SwC4oK1NlSgFx_cjKYbY0NychCG0XxLrl5XUoR79va4-9HGRMDYaTFRMutwMzFF_4iCbpn3RHl-qu9_RAabJrsQkeCmYYXaQKLt_aVVfrBMQWOwJDvCuTaeJcRGJefKmNdc-aM8mqBjZX9RIocD_hp5ADxY9HZdBFtGz7OAofgM2ZqVeJPkvNKfQ";
pub const TEST_SIG_EXPIRED = "Ctiud6VRvF54eited-UOq6HEiKZWNdhPli_w_rsFLmS6bmeDr2xuXlag6HgZLCnOc1mHsoJGGqeojZ8xt2SVt6JHjxXxV6KhP6orw4FPgmPKzyZw2zFWGmi3M0IUSv9CzsaaWnoj5KdLL9DWzF--EpMDddqaEMBLolxuMV_uO0m6Fji6lJikVZaPTFJ0YMzcMdkvh4iZ9_y2fGYvjUSPnbNw-3eq4P4tlUq2n_6ES17zLGIF55cRoUo7v-audITTd9AVwp0Y3-_PXq-yAJEPTBhysG1iYiKMrf9x_r1h6g2rKCFQ_7k48K-o_rUPSFVaU0Q3TXG3CDoMjmAma0LN6A";

pub const TEST_VALID_TOKEN = TEST_HEADER ++ "." ++ TEST_PAYLOAD_VALID ++ "." ++ TEST_SIG_VALID;
pub const TEST_EXPIRED_TOKEN = TEST_HEADER ++ "." ++ TEST_PAYLOAD_EXPIRED ++ "." ++ TEST_SIG_EXPIRED;

const std = @import("std");

test "fixture key set is well-formed JSON pinned to the shared kid (Dimension 6.4)" {
    var parsed = try std.json.parseFromSlice(std.json.Value, std.testing.allocator, TEST_JWKS, .{});
    defer parsed.deinit();
    const key = parsed.value.object.get("keys").?.array.items[0].object;
    try std.testing.expectEqualStrings(TEST_KID, key.get("kid").?.string);
    try std.testing.expectEqualStrings(TEST_RSA_N, key.get("n").?.string);
}
