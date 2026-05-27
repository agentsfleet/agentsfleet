// Pepper-section tests for ServeConfig.load + loader.loadAuthPeppers.
//
// Carved out of runtime_loader_test.zig to keep that file under the 350-line
// FLL cap. Same harness shape (clearAllRuntimeEnv / setTestEnv / unsetTestEnv)
// duplicated here so this file is self-contained — the harness is small
// enough that the duplication is cheaper than threading it through a shared
// helper module just for two test files. test "..." names are deliberately
// milestone-free (RULE TST-NAM).

const std = @import("std");
const runtime = @import("runtime.zig");
const loader = @import("runtime_loader.zig");

const ServeConfig = runtime.ServeConfig;
const ValidationError = runtime.ValidationError;

const test_encryption_master_key = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const test_session_code_pepper = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const test_audit_log_pepper = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

const ALL_RUNTIME_ENV_VARS = [_][]const u8{
    "PORT",                       "API_HTTP_THREADS",
    "API_HTTP_WORKERS",           "API_MAX_CLIENTS",
    "API_MAX_IN_FLIGHT_REQUESTS", "READY_MAX_QUEUE_DEPTH",
    "READY_MAX_QUEUE_AGE_MS",     "OIDC_JWKS_URL",
    "OIDC_ISSUER",                "OIDC_AUDIENCE",
    "OIDC_PROVIDER",              "API_KEY",
    "ENCRYPTION_MASTER_KEY",      "AUTH_SESSION_CODE_PEPPER",
    "AUDIT_LOG_PEPPER",           "APP_URL",
    "API_URL",
};

fn clearAllRuntimeEnv() void {
    for (ALL_RUNTIME_ENV_VARS) |name| std.posix.unsetenv(name);
}

fn setTestEnv(env_pairs: []const [2][]const u8) !void {
    clearAllRuntimeEnv();
    for (env_pairs) |entry| try std.posix.setenv(entry[0], entry[1], true);
}

fn unsetTestEnv(env_pairs: []const [2][]const u8) void {
    for (env_pairs) |entry| std.posix.unsetenv(entry[0]);
}

test "loadAuthPeppers rejects missing session-code pepper" {
    const env_pairs = [_][2][]const u8{
        .{ "AUDIT_LOG_PEPPER", test_audit_log_pepper },
    };
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);
    try std.testing.expectError(ValidationError.MissingAuthSessionCodePepper, loader.loadAuthPeppers(std.testing.allocator));
}

test "loadAuthPeppers rejects missing audit-log pepper" {
    const env_pairs = [_][2][]const u8{
        .{ "AUTH_SESSION_CODE_PEPPER", test_session_code_pepper },
    };
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);
    try std.testing.expectError(ValidationError.MissingAuditLogPepper, loader.loadAuthPeppers(std.testing.allocator));
}

test "loadAuthPeppers rejects short session-code pepper" {
    const env_pairs = [_][2][]const u8{
        .{ "AUTH_SESSION_CODE_PEPPER", "tooshort" },
        .{ "AUDIT_LOG_PEPPER", test_audit_log_pepper },
    };
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);
    try std.testing.expectError(ValidationError.InvalidAuthSessionCodePepper, loader.loadAuthPeppers(std.testing.allocator));
}

test "loadAuthPeppers rejects non-hex session-code pepper" {
    const env_pairs = [_][2][]const u8{
        .{ "AUTH_SESSION_CODE_PEPPER", "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz" },
        .{ "AUDIT_LOG_PEPPER", test_audit_log_pepper },
    };
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);
    try std.testing.expectError(ValidationError.InvalidAuthSessionCodePepper, loader.loadAuthPeppers(std.testing.allocator));
}

test "loadAuthPeppers rejects non-hex audit-log pepper" {
    const env_pairs = [_][2][]const u8{
        .{ "AUTH_SESSION_CODE_PEPPER", test_session_code_pepper },
        .{ "AUDIT_LOG_PEPPER", "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy" },
    };
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);
    try std.testing.expectError(ValidationError.InvalidAuditLogPepper, loader.loadAuthPeppers(std.testing.allocator));
}

test "loadAuthPeppers accepts two distinct 64-hex peppers" {
    const env_pairs = [_][2][]const u8{
        .{ "AUTH_SESSION_CODE_PEPPER", test_session_code_pepper },
        .{ "AUDIT_LOG_PEPPER", test_audit_log_pepper },
    };
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);

    const cfg = try loader.loadAuthPeppers(std.testing.allocator);
    defer loader.freeAuthPeppers(std.testing.allocator, cfg);
    try std.testing.expectEqualStrings(test_session_code_pepper, cfg.session_code_pepper);
    try std.testing.expectEqualStrings(test_audit_log_pepper, cfg.audit_log_pepper);
}

test "ServeConfig.load partial-build frees prior sections when peppers rejected (RULE OWN)" {
    // Mirrors the encryption-rejected partial-build test in runtime_loader_test.zig.
    // Loads valid OIDC + API_KEY + encryption (each allocates), then forces
    // loadAuthPeppers to fail via a missing AUDIT_LOG_PEPPER. std.testing.allocator
    // panics on any leak; clean exit proves the errdefer chain is intact through
    // the new pepper section.
    const env_pairs = [_][2][]const u8{
        .{ "OIDC_JWKS_URL", "https://idp.example.com/.well-known/jwks.json" },
        .{ "OIDC_ISSUER", "https://idp.example.com/" },
        .{ "OIDC_AUDIENCE", "zombied-prod" },
        .{ "API_KEY", "dev-key" },
        .{ "ENCRYPTION_MASTER_KEY", test_encryption_master_key },
        .{ "AUTH_SESSION_CODE_PEPPER", test_session_code_pepper },
        // AUDIT_LOG_PEPPER deliberately omitted
    };
    try setTestEnv(&env_pairs);
    defer unsetTestEnv(&env_pairs);
    try std.testing.expectError(ValidationError.MissingAuditLogPepper, ServeConfig.load(std.testing.allocator));
}
