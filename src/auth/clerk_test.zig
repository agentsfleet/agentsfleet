const std = @import("std");
const clerk = @import("clerk.zig");

const VerifyError = clerk.VerifyError;
const Verifier = clerk.Verifier;

const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","kid":"test-kid-static","use":"sig","alg":"RS256","n":"kEge9Llezx-onM-jdO1fw85yTFmDDHWaZVdihVqMVAvRDGFvHbyoPrp5F-ZaDTqVEd1_pH12HM3abE6HRyYwSRxPcSKf2GlGWBVPtFbidOezLupgspHs8-yXBFKkGQEGBTWspJ4Obd0g9u1EX-cQqzy-lXiGd8gt1oK8Rxx5YBohNbaQMs5dbJ61J9c0afrG0dx-xOOx2tb95izx_m-sB83-aj7mX_r3ClpbZYcOY8ZKA3QNwR9tattkTiowpgzBZ0PGw5wuzrQayjWQRooolW4kzYMVWOI5K4GVPoabBDZDPs2nfet290iFHkNRu8cc2xPDmty0cDIhbS9Mq33qsQ","e":"AQAB"}]}
;
const TEST_HEADER = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9";
const TEST_PAYLOAD_VALID = "eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImlhdCI6MTcwNDA2NzIwMCwib3JnX2lkIjoib3JnXzEiLCJtZXRhZGF0YSI6eyJ0ZW5hbnRfaWQiOiJ0ZW5hbnRfYSJ9LCJleHAiOjQxMDI0NDQ4MDB9";
const TEST_PAYLOAD_EXPIRED = "eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImlhdCI6MTcwNDA2NzIwMCwib3JnX2lkIjoib3JnXzEiLCJtZXRhZGF0YSI6eyJ0ZW5hbnRfaWQiOiJ0ZW5hbnRfYSJ9LCJleHAiOjE3MDQwNjczMDB9";
const TEST_SIG_VALID = "R5EaetratAEMN3VcDRDyR3KM9dKU3FYGEvzajPdmMUB_3T3qE0G0xZ_IoqyNilvjuMcbdSF-YQL1ylcMPTyBeFUWYAUlMjWBju-Bt3FF0Abqdte5-a64oPb_Ev0ogZyJcI8DDt9yT4kUjH7S2jp4fu9hQaEDMW_6tcASagCHTIjw2h0A41_Y8PI4CrgglIFqEKGim5PUEWM_KzZxs9pjv7-_HsZTovfZTcKeiJkGiFQvyR3oKfudvjLNyyGtdYKiSjfOWtLfJkxGt0CKPkbDbrnj_cSmwCt-X_v_OmG5vm07h7iDKrKhXiav0Djn7W3zZ8EcwjhlvMSsKZ3Uy9Nk2g";
const TEST_SIG_EXPIRED = "CCjR252liw1fHwmo4kBmHH0nw1uPUBtibZx9BSPKPdzU_4oDmSrJyFP4LJtd9THDIVV8JyE5r1I9a8nuyLe66Wfr_N_tiiNAzYQ0voN_B2AQ-iy8DHhVAJibflv5eaGRXxh4pfn7uV3vY1ZGGDxwyjOXWPy_ULwSwtaDGDQNeWWYgVaaKp1B0-l__oIiMmRgsCiMOE6qyU2SFCQKG05vF54fgg7Pp4hpOgR9guE-rYKoLo39qE0RJvnaf5MTz2WbsPRxrvGurJ1lgnPrxGSXDMT2xJATkof6hP3Hv3QuSRlfCQwLEvlHKZG5ANpe7dxQ00KGf3RJiv0ly9mPapsD5g";
const TEST_VALID_TOKEN = TEST_HEADER ++ "." ++ TEST_PAYLOAD_VALID ++ "." ++ TEST_SIG_VALID;
const TEST_EXPIRED_TOKEN = TEST_HEADER ++ "." ++ TEST_PAYLOAD_EXPIRED ++ "." ++ TEST_SIG_EXPIRED;

fn makeVerifier(audience: []const u8) Verifier {
    return Verifier.init(std.testing.allocator, .{
        .jwks_url = "https://clerk.dev.usezombie.com/.well-known/jwks.json",
        .issuer = "https://clerk.dev.usezombie.com",
        .audience = audience,
        .inline_jwks_json = TEST_JWKS,
    });
}

test "verifyAuthorization validates RS256 token and extracts tenant" {
    var verifier = makeVerifier("https://api.usezombie.com");
    defer verifier.deinit();

    const principal = try verifier.verifyAuthorization(std.testing.allocator, "Bearer " ++ TEST_VALID_TOKEN);
    defer {
        std.testing.allocator.free(principal.subject);
        std.testing.allocator.free(principal.issuer);
        if (principal.tenant_id) |v| std.testing.allocator.free(v);
        if (principal.org_id) |v| std.testing.allocator.free(v);
        if (principal.workspace_id) |v| std.testing.allocator.free(v);
    }

    try std.testing.expectEqualStrings("user_test", principal.subject);
    try std.testing.expectEqualStrings("https://clerk.dev.usezombie.com", principal.issuer);
    try std.testing.expectEqualStrings("tenant_a", principal.tenant_id.?);
    try std.testing.expectEqualStrings("org_1", principal.org_id.?);
    try std.testing.expect(principal.workspace_id == null);
}

test "verifyAuthorization rejects expired token" {
    var verifier = makeVerifier("https://api.usezombie.com");
    defer verifier.deinit();

    try std.testing.expectError(VerifyError.TokenExpired, verifier.verifyAuthorization(std.testing.allocator, "Bearer " ++ TEST_EXPIRED_TOKEN));
}

test "verifyAuthorization rejects audience mismatch" {
    var verifier = makeVerifier("https://api.other.example");
    defer verifier.deinit();

    try std.testing.expectError(VerifyError.AudienceMismatch, verifier.verifyAuthorization(std.testing.allocator, "Bearer " ++ TEST_VALID_TOKEN));
}
