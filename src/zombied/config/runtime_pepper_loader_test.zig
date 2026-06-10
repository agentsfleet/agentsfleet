// Pepper-section tests for ServeConfig.load + loader.loadAuthPeppers.
//
// Carved out of runtime_loader_test.zig to keep that file under the 350-line
// FLL cap. Each test builds a hermetic env map via `common.env.fromPairs`
// (the Zig 0.16 env-DI seam). test "..." names are deliberately
// milestone-free (RULE TST-NAM).

const std = @import("std");
const common = @import("common");
const runtime = @import("runtime.zig");
const loader = @import("runtime_loader.zig");

const ServeConfig = runtime.ServeConfig;
const ValidationError = runtime.ValidationError;

const test_encryption_master_key = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const test_session_code_pepper = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const test_audit_log_pepper = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

fn envOf(pairs: []const [2][]const u8) !common.env.Map {
    return common.env.fromPairs(std.testing.allocator, pairs);
}

test "loadAuthPeppers rejects missing session-code pepper" {
    var env_map = try envOf(&.{
        .{ "AUDIT_LOG_PEPPER", test_audit_log_pepper },
    });
    defer env_map.deinit();
    try std.testing.expectError(ValidationError.MissingAuthSessionCodePepper, loader.loadAuthPeppers(&env_map, std.testing.allocator));
}

test "loadAuthPeppers rejects missing audit-log pepper" {
    var env_map = try envOf(&.{
        .{ "AUTH_SESSION_CODE_PEPPER", test_session_code_pepper },
    });
    defer env_map.deinit();
    try std.testing.expectError(ValidationError.MissingAuditLogPepper, loader.loadAuthPeppers(&env_map, std.testing.allocator));
}

test "loadAuthPeppers rejects short session-code pepper" {
    var env_map = try envOf(&.{
        .{ "AUTH_SESSION_CODE_PEPPER", "tooshort" },
        .{ "AUDIT_LOG_PEPPER", test_audit_log_pepper },
    });
    defer env_map.deinit();
    try std.testing.expectError(ValidationError.InvalidAuthSessionCodePepper, loader.loadAuthPeppers(&env_map, std.testing.allocator));
}

test "loadAuthPeppers rejects non-hex session-code pepper" {
    var env_map = try envOf(&.{
        .{ "AUTH_SESSION_CODE_PEPPER", "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz" },
        .{ "AUDIT_LOG_PEPPER", test_audit_log_pepper },
    });
    defer env_map.deinit();
    try std.testing.expectError(ValidationError.InvalidAuthSessionCodePepper, loader.loadAuthPeppers(&env_map, std.testing.allocator));
}

test "loadAuthPeppers rejects non-hex audit-log pepper" {
    var env_map = try envOf(&.{
        .{ "AUTH_SESSION_CODE_PEPPER", test_session_code_pepper },
        .{ "AUDIT_LOG_PEPPER", "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy" },
    });
    defer env_map.deinit();
    try std.testing.expectError(ValidationError.InvalidAuditLogPepper, loader.loadAuthPeppers(&env_map, std.testing.allocator));
}

test "loadAuthPeppers accepts two distinct 64-hex peppers" {
    var env_map = try envOf(&.{
        .{ "AUTH_SESSION_CODE_PEPPER", test_session_code_pepper },
        .{ "AUDIT_LOG_PEPPER", test_audit_log_pepper },
    });
    defer env_map.deinit();

    const cfg = try loader.loadAuthPeppers(&env_map, std.testing.allocator);
    defer loader.freeAuthPeppers(std.testing.allocator, cfg);
    try std.testing.expectEqualStrings(test_session_code_pepper, cfg.session_code_pepper);
    try std.testing.expectEqualStrings(test_audit_log_pepper, cfg.audit_log_pepper);
}

test "ServeConfig.load partial-build frees prior sections when peppers rejected (RULE OWN)" {
    // Mirrors the encryption-rejected partial-build test in runtime_loader_test.zig.
    // Loads valid OIDC + encryption (each allocates), then forces
    // loadAuthPeppers to fail via a missing AUDIT_LOG_PEPPER. std.testing.allocator
    // panics on any leak; clean exit proves the errdefer chain is intact through
    // the pepper section.
    var env_map = try envOf(&.{
        .{ "OIDC_JWKS_URL", "https://idp.example.com/.well-known/jwks.json" },
        .{ "OIDC_ISSUER", "https://idp.example.com/" },
        .{ "OIDC_AUDIENCE", "zombied-prod" },
        .{ "ENCRYPTION_MASTER_KEY", test_encryption_master_key },
        .{ "AUTH_SESSION_CODE_PEPPER", test_session_code_pepper },
        // AUDIT_LOG_PEPPER deliberately omitted
    });
    defer env_map.deinit();
    try std.testing.expectError(ValidationError.MissingAuditLogPepper, ServeConfig.load(&env_map, std.testing.allocator));
}
