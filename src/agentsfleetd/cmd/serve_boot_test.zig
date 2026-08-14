//! serve_boot's success prologue — the half a live boot exercises and no test
//! did. The failure arms call `std.process.exit` and stay untestable
//! in-process by design; what IS pinned here is that a correct environment
//! boots: args parse, env gate, config load, Key-Encryption Key (KEK)
//! install, the disabled-OIDC path, and the registry wiring serve.run hands
//! to every middleware chain.

const std = @import("std");
const common = @import("common");

const serve_boot = @import("serve_boot.zig");
const api_key_lookup = @import("api_key_lookup.zig");
const serve_runner_lookup = @import("serve_runner_lookup.zig");

const ALLOC = std.testing.allocator;

const MASTER_KEY = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SESSION_PEPPER = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const AUDIT_PEPPER = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

test "parseArgsOrExit: no overrides is null, an explicit port parses" {
    const bare = [_][:0]const u8{ "agentsfleetd", "serve" };
    try std.testing.expect(serve_boot.parseArgsOrExit(&bare) == null);

    const with_port = [_][:0]const u8{ "agentsfleetd", "serve", "--port", "9099" };
    try std.testing.expectEqual(@as(?u16, 9099), serve_boot.parseArgsOrExit(&with_port));
}

test "enforceEnvOrExit passes a complete API environment" {
    var env = try common.env.fromPairs(ALLOC, &.{
        .{ "DATABASE_URL_API", "postgres://u:p@localhost:5432/db" },
        .{ "REDIS_URL_API", "rediss://:pw@localhost:6379" },
    });
    defer env.deinit();
    // The failure arms exit the process; reaching the next line IS the assert.
    serve_boot.enforceEnvOrExit(&env, ALLOC);
}

test "loadServeConfigOrExit returns the validated config on a correct env" {
    var env = try common.env.fromPairs(ALLOC, &.{
        .{ "OIDC_JWKS_URL", "https://idp.example.com/.well-known/jwks.json" },
        .{ "OIDC_ISSUER", "https://idp.example.com" },
        .{ "OIDC_PROVIDER", "custom" },
        .{ "ENCRYPTION_MASTER_KEY", MASTER_KEY },
        .{ "AUTH_SESSION_CODE_PEPPER", SESSION_PEPPER },
        .{ "AUDIT_LOG_PEPPER", AUDIT_PEPPER },
    });
    defer env.deinit();

    var cfg = serve_boot.loadServeConfigOrExit(&env, ALLOC);
    defer cfg.deinit();
    try std.testing.expect(cfg.oidc_enabled);

    // The KEK install reads the already-validated value — the only in-process
    // arm of setKekOrExit (its failure half exits).
    serve_boot.setKekOrExit(cfg.encryption_master_key);
}

test "buildRegistry wires every middleware to its boot-owned host" {
    // SAFETY: buildRegistry stores the ctx pointers; no lookup ever runs here,
    // so the pool member is never read.
    var api_ctx = api_key_lookup.Ctx{ .pool = undefined };
    // SAFETY: same — stored, never dereferenced in this test.
    var runner_ctx = serve_runner_lookup.Ctx{ .pool = undefined };

    const reg = serve_boot.buildRegistry(null, &api_ctx, &runner_ctx, "signing-secret");

    try std.testing.expect(reg.bearer_or_api_key.verifier == null);
    try std.testing.expectEqual(@as(*anyopaque, @ptrCast(&api_ctx)), @as(*anyopaque, @ptrCast(reg.tenant_api_key_mw.host)));
    try std.testing.expectEqual(@as(*anyopaque, @ptrCast(&runner_ctx)), @as(*anyopaque, @ptrCast(reg.runner_bearer_mw.host)));
    try std.testing.expectEqualStrings("signing-secret", reg.webhook_hmac_mw.secret);
}
