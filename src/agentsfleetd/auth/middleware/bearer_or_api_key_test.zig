//! Behavioural tests for `bearer_or_api_key` — the routing decision that
//! picks a credential class for a Bearer value, and the OIDC path it falls
//! through to. Split from the middleware so that file holds its length cap;
//! everything the tests reach is part of that module's public surface.

const std = @import("std");
const httpz = @import("httpz");

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const errors = @import("errors.zig");
const oidc = @import("../oidc.zig");
const principal_mod = @import("../principal.zig");
const cli_credential_mod = @import("cli_credential.zig");
const bearer_or_api_key = @import("bearer_or_api_key.zig");

const AuthCtx = auth_ctx.AuthCtx;
const BearerOrApiKey = bearer_or_api_key.BearerOrApiKey;
const CliCredential = bearer_or_api_key.CliCredential;
const testing = std.testing;

// Single-sourced in ../jwks_test_fixtures.zig (Dimension 6.4).
const test_fx = @import("../jwks_test_fixtures.zig");
const TEST_JWKS = test_fx.TEST_JWKS;
const TEST_VALID_TOKEN = test_fx.TEST_VALID_TOKEN;

const test_fixtures = struct {
    var last_code: []const u8 = "";
    var write_count: usize = 0;

    fn reset() void {
        last_code = "";
        write_count = 0;
    }

    fn writeError(_: *httpz.Response, code: []const u8, _: []const u8, _: []const u8) void {
        last_code = code;
        write_count += 1;
    }
};

fn makeVerifier() error{OutOfMemory}!oidc.Verifier {
    return oidc.Verifier.init(testing.allocator, .{
        .provider = .clerk,
        .jwks_url = "https://clerk.dev.agentsfleet.net/.well-known/jwks.json",
        .issuer = "https://clerk.dev.agentsfleet.net",
        .audience = "https://api.agentsfleet.net",
        .inline_jwks_json = TEST_JWKS,
    });
}

fn runOne(mw: *BearerOrApiKey, ht: anytype) !struct { outcome: chain.Outcome, ctx: AuthCtx } {
    var ctx = AuthCtx{
        .alloc = testing.allocator,
        .res = ht.res,
        .req_id = "req_test",
        .write_error = test_fixtures.writeError,
    };
    const outcome = try mw.execute(&ctx, ht.req);
    return .{ .outcome = outcome, .ctx = ctx };
}

test "bearer_or_api_key routes a valid JWT to the OIDC path" {
    test_fixtures.reset();
    var verifier = try makeVerifier();
    defer verifier.deinit();

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer " ++ TEST_VALID_TOKEN);

    var mw = BearerOrApiKey{ .verifier = &verifier };
    const result = try runOne(&mw, &ht);
    defer if (result.ctx.principal) |p| {
        if (p.user_id) |v| testing.allocator.free(v);
        if (p.tenant_id) |v| testing.allocator.free(v);
        if (p.workspace_scope_id) |v| testing.allocator.free(v);
    };

    try testing.expectEqual(chain.Outcome.next, result.outcome);
    try testing.expect(result.ctx.principal != null);
    try testing.expectEqual(principal_mod.AuthMode.jwt_oidc, result.ctx.principal.?.mode);
    try testing.expectEqualStrings("user_test", result.ctx.principal.?.user_id.?);
}

test "bearer_or_api_key short-circuits with 401 when Authorization header is missing" {
    test_fixtures.reset();
    var verifier = try makeVerifier();
    defer verifier.deinit();

    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    var mw = BearerOrApiKey{ .verifier = &verifier };
    const result = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.short_circuit, result.outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, test_fixtures.last_code);
}

test "bearer_or_api_key short-circuits with 401 when no verifier is configured" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer something-else");

    var mw = BearerOrApiKey{ .verifier = null };
    const result = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.short_circuit, result.outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, test_fixtures.last_code);
}

// ── Command-line credential routing ──────────────────────────────────────
//
// Behavioural coverage of the credential middleware itself lives in
// `cli_credential_test.zig`. What is proved here is only the routing decision
// this file owns: which of the three credential classes a Bearer value reaches.

const cli_fx = struct {
    const SUBJECT = "user_2aXyTest";
    const CLAIM = "fleet:read";
    const CREDENTIAL = cli_credential_mod.CLI_CREDENTIAL_PREFIX ++ "a" ** 64;

    fn lookup(_: *anyopaque, alloc: std.mem.Allocator, _: []const u8) anyerror!?cli_credential_mod.LookupResult {
        return .{
            .credential_id = try alloc.dupe(u8, "cred_1"),
            .user_id = try alloc.dupe(u8, "01920000-0000-7000-8000-000000000001"),
            .tenant_id = try alloc.dupe(u8, "01920000-0000-7000-8000-0000000000t1"),
            .deployment = try alloc.dupe(u8, "https://api.agentsfleet.net"),
            .revoked = false,
            .oidc_subject = try alloc.dupe(u8, SUBJECT),
        };
    }

    fn resolveScopes(_: *anyopaque, alloc: std.mem.Allocator, _: []const u8) anyerror![]const u8 {
        return alloc.dupe(u8, CLAIM);
    }

    fn attach(mw: *BearerOrApiKey, cli: *CliCredential, host: *usize) void {
        cli.* = .{ .host = host, .lookup = lookup, .scope_host = host, .resolveScopes = resolveScopes };
        mw.cli_credential = cli;
    }
};

test "bearer_or_api_key routes an afc_ credential to the user-principal path" {
    test_fixtures.reset();
    var verifier = try makeVerifier();
    defer verifier.deinit();

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer " ++ cli_fx.CREDENTIAL);

    var host: usize = 0;
    // SAFETY: attach() writes every field before the registry pointer is read.
    var cli: CliCredential = undefined;
    var mw = BearerOrApiKey{ .verifier = &verifier };
    cli_fx.attach(&mw, &cli, &host);

    const result = try runOne(&mw, &ht);
    defer if (result.ctx.principal) |p| {
        if (p.user_id) |v| testing.allocator.free(v);
        if (p.tenant_id) |v| testing.allocator.free(v);
    };

    try testing.expectEqual(chain.Outcome.next, result.outcome);
    // A credential resolves to a person, never to the whole organisation.
    try testing.expectEqual(principal_mod.AuthMode.cli_credential, result.ctx.principal.?.mode);
    try testing.expectEqualStrings(cli_fx.SUBJECT, result.ctx.principal.?.user_id.?);
    try testing.expect(result.ctx.principal.?.scopes.contains(.fleet_read));
}

test "bearer_or_api_key resolves an afc_ credential with no identity provider configured" {
    // A self-contained credential class must not depend on the verifier being
    // present — otherwise a deployment with the provider disabled refuses a
    // credential it could have resolved on its own.
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer " ++ cli_fx.CREDENTIAL);

    var host: usize = 0;
    // SAFETY: attach() writes every field before the registry pointer is read.
    var cli: CliCredential = undefined;
    var mw = BearerOrApiKey{ .verifier = null };
    cli_fx.attach(&mw, &cli, &host);

    const result = try runOne(&mw, &ht);
    defer if (result.ctx.principal) |p| {
        if (p.user_id) |v| testing.allocator.free(v);
        if (p.tenant_id) |v| testing.allocator.free(v);
    };

    try testing.expectEqual(chain.Outcome.next, result.outcome);
    try testing.expectEqual(principal_mod.AuthMode.cli_credential, result.ctx.principal.?.mode);
}

test "an unprefixed token still reaches the identity provider once the credential path is wired" {
    // Wiring the credential branch must not swallow the JWT path — the same
    // instance serves both, and a prefix test that matched too broadly would
    // take every dashboard call down with it.
    test_fixtures.reset();
    var verifier = try makeVerifier();
    defer verifier.deinit();

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer " ++ TEST_VALID_TOKEN);

    var host: usize = 0;
    // SAFETY: attach() writes every field before the registry pointer is read.
    var cli: CliCredential = undefined;
    var mw = BearerOrApiKey{ .verifier = &verifier };
    cli_fx.attach(&mw, &cli, &host);

    const result = try runOne(&mw, &ht);
    defer if (result.ctx.principal) |p| {
        if (p.user_id) |v| testing.allocator.free(v);
        if (p.tenant_id) |v| testing.allocator.free(v);
        if (p.workspace_scope_id) |v| testing.allocator.free(v);
    };

    try testing.expectEqual(principal_mod.AuthMode.jwt_oidc, result.ctx.principal.?.mode);
}

test "bearer_or_api_key short-circuits with 503 when JWKS fetch fails" {
    test_fixtures.reset();
    var verifier = try oidc.Verifier.init(testing.allocator, .{
        .provider = .clerk,
        .jwks_url = "http://127.0.0.1:1/unreachable.json",
        .issuer = "https://clerk.dev.agentsfleet.net",
        .audience = "https://api.agentsfleet.net",
    });
    defer verifier.deinit();

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer " ++ TEST_VALID_TOKEN);

    var mw = BearerOrApiKey{ .verifier = &verifier };
    const result = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.short_circuit, result.outcome);
    try testing.expectEqualStrings(errors.ERR_AUTH_UNAVAILABLE, test_fixtures.last_code);
}
