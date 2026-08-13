//! Behavioural tests for the boot-stage helpers.
//!
//! Every `*OrExit` helper is exercised on its success path only — the failure
//! arms call `std.process.exit`, which would take the test runner with them.
//! That boundary is deliberate in serve_boot (callers decide nothing; a broken
//! boot input has no recovery), so the failure arms stay proven by the
//! underlying modules' own tests (`serve_args`, `env_vars`, `runtime_config`),
//! which return errors instead of exiting.

const std = @import("std");
const constants = @import("common");
const serve_boot = @import("serve_boot.zig");
const api_key_lookup = @import("api_key_lookup.zig");
const cli_credential_lookup = @import("cli_credential_lookup.zig");
const clerk_scope_resolver = @import("../auth/clerk_scope_resolver.zig");
const serve_runner_lookup = @import("serve_runner_lookup.zig");

const testing = std.testing;

/// A 32-byte key, spelled in hex by repetition so no high-entropy literal
/// enters the tree for a secret scanner to flag.
const VALID_KEK_HEX = "ab" ** 32;

/// The smallest environment `ServeConfig.load` accepts: OIDC is mandatory
/// (the issuer is the enable-gate), and the encryption key plus both peppers
/// are `required*` loads. Everything else defaults.
fn minimalServeEnv(alloc: std.mem.Allocator) !constants.env.Map {
    return constants.env.fromPairs(alloc, &.{
        .{ "OIDC_ISSUER", "https://clerk.test.agentsfleet.net" },
        .{ "OIDC_AUDIENCE", "https://api.agentsfleet.net" },
        .{ "ENCRYPTION_MASTER_KEY", VALID_KEK_HEX },
        .{ "AUTH_SESSION_CODE_PEPPER", "cd" ** 32 },
        .{ "AUDIT_LOG_PEPPER", "ef" ** 32 },
    });
}

test "parseArgsOrExit returns null when serve is given no overrides" {
    const argv = [_][:0]const u8{ "agentsfleetd", "serve" };
    try testing.expectEqual(@as(?u16, null), serve_boot.parseArgsOrExit(&argv));
}

test "parseArgsOrExit returns the port override" {
    const argv = [_][:0]const u8{ "agentsfleetd", "serve", "--port", "8123" };
    try testing.expectEqual(@as(?u16, 8123), serve_boot.parseArgsOrExit(&argv));
}

test "enforceEnvOrExit passes a complete API environment through" {
    const alloc = testing.allocator;
    var env = try constants.env.fromPairs(alloc, &.{
        .{ "DATABASE_URL_API", "postgres://api:pw@db.local:5432/agentsfleetdb" },
        .{ "REDIS_URL_API", "rediss://api:pw@cache.local:6379" },
    });
    defer env.deinit();
    // Reaching the return at all is the assertion — the failure arm exits.
    serve_boot.enforceEnvOrExit(&env, alloc);
}

test "loadServeConfigOrExit loads the minimal valid environment" {
    const alloc = testing.allocator;
    var env = try minimalServeEnv(alloc);
    defer env.deinit();

    var cfg = serve_boot.loadServeConfigOrExit(&env, alloc);
    defer cfg.deinit();

    try testing.expect(cfg.oidc_enabled);
    try testing.expectEqualStrings("https://clerk.test.agentsfleet.net", cfg.oidc_issuer.?);
    try testing.expectEqualStrings(VALID_KEK_HEX, cfg.encryption_master_key);
}

test "setKekOrExit accepts a well-formed key" {
    // Reaching the return is the assertion; a malformed key exits the process.
    serve_boot.setKekOrExit(VALID_KEK_HEX);
}

test "initOidc answers null when OIDC is disabled" {
    const alloc = testing.allocator;
    var env = try minimalServeEnv(alloc);
    defer env.deinit();
    var cfg = serve_boot.loadServeConfigOrExit(&env, alloc);
    defer cfg.deinit();

    cfg.oidc_enabled = false;
    try testing.expect((try serve_boot.initOidc(alloc, &cfg)) == null);
}

test "initOidc builds a verifier for an enabled config without touching the network" {
    // Verifier.init's error set is OutOfMemory alone — the JWKS fetch is lazy,
    // which is what makes the boot helper provable here.
    const alloc = testing.allocator;
    var env = try minimalServeEnv(alloc);
    defer env.deinit();
    var cfg = serve_boot.loadServeConfigOrExit(&env, alloc);
    defer cfg.deinit();

    var verifier = (try serve_boot.initOidc(alloc, &cfg)) orelse return error.TestExpectedEqual;
    defer verifier.deinit();
}

test "buildRegistry wires every middleware to its lookup and the shared resolver" {
    // The pointers are stored, never dereferenced, so placeholder hosts are
    // safe — what this pins is the WIRING: both credential classes must hold
    // the same resolver instance and the same resolve function, or they grow
    // separate caches with separate staleness stories.
    var api_ctx: api_key_lookup.Ctx = undefined;
    var cli_ctx: cli_credential_lookup.Ctx = undefined;
    var resolver: clerk_scope_resolver.ScopeResolver = undefined;
    var runner_ctx: serve_runner_lookup.Ctx = undefined;

    const reg = serve_boot.buildRegistry(.{
        .verifier = null,
        .api_key_lookup_ctx = &api_ctx,
        .cli_credential_lookup_ctx = &cli_ctx,
        .scope_resolver = &resolver,
        .runner_lookup_ctx = &runner_ctx,
        .approval_signing_secret = "test-secret",
    });

    try testing.expectEqual(@as(*anyopaque, @ptrCast(&resolver)), reg.tenant_api_key_mw.scope_host);
    try testing.expectEqual(@as(*anyopaque, @ptrCast(&resolver)), reg.cli_credential_mw.scope_host);
    try testing.expectEqual(reg.tenant_api_key_mw.resolveScopes, reg.cli_credential_mw.resolveScopes);
    try testing.expectEqual(@as(*anyopaque, @ptrCast(&api_ctx)), reg.tenant_api_key_mw.host);
    try testing.expectEqual(@as(*anyopaque, @ptrCast(&cli_ctx)), reg.cli_credential_mw.host);
    try testing.expectEqual(@as(*anyopaque, @ptrCast(&runner_ctx)), reg.runner_bearer_mw.host);
    try testing.expectEqualStrings("test-secret", reg.webhook_hmac_mw.secret);
    try testing.expect(reg.bearer_or_api_key.verifier == null);
}
