//! Behavioural tests for the `cli_credential` middleware.
//!
//! Split from the module under test to hold it under the file-length limit,
//! matching the `*_test.zig` siblings already in this directory. Everything
//! here drives the public surface — `CliCredential`, `LookupResult`, and the
//! two injected callbacks — so the middleware needs neither a datastore nor
//! the identity provider to be provable.

const std = @import("std");
const httpz = @import("httpz");

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const errors = @import("errors.zig");
const principal_mod = @import("../principal.zig");
const scopes = @import("../scopes.zig");
const mw_mod = @import("cli_credential.zig");

const AuthCtx = auth_ctx.AuthCtx;
const CliCredential = mw_mod.CliCredential;
const LookupResult = mw_mod.LookupResult;

const auth_codes = @import("auth_codes");
const ERR_CLI_CREDENTIAL_REVOKED = auth_codes.ERR_CLI_CREDENTIAL_REVOKED;

const testing = std.testing;

/// Injected lookup standing in for the datastore, so the middleware's branches
/// are provable inside the portability wall.
const test_lookup = struct {
    var last_code: []const u8 = "";
    var row: ?LookupResult = null;
    var fail_lookup: bool = false;
    var called_with_hash: [64]u8 = undefined;
    var call_count: usize = 0;
    var claim: []const u8 = "";
    var fail_scopes: bool = false;
    var scope_calls: usize = 0;
    var scoped_subject: []const u8 = "";

    fn reset() void {
        last_code = "";
        row = null;
        fail_lookup = false;
        call_count = 0;
        claim = "";
        fail_scopes = false;
        scope_calls = 0;
        scoped_subject = "";
    }

    fn writeError(_: *httpz.Response, code: []const u8, _: []const u8, _: []const u8) void {
        last_code = code;
    }

    fn lookup(_: *anyopaque, alloc: std.mem.Allocator, hash_hex: []const u8) anyerror!?LookupResult {
        call_count += 1;
        @memcpy(&called_with_hash, hash_hex[0..64]);
        if (fail_lookup) return error.DbUnavailable;
        const src = row orelse return null;
        return .{
            .credential_id = try alloc.dupe(u8, src.credential_id),
            .user_id = try alloc.dupe(u8, src.user_id),
            .tenant_id = try alloc.dupe(u8, src.tenant_id),
            .deployment = try alloc.dupe(u8, src.deployment),
            .revoked = src.revoked,
            .oidc_subject = try alloc.dupe(u8, src.oidc_subject),
        };
    }

    fn resolveScopes(_: *anyopaque, alloc: std.mem.Allocator, oidc_subject: []const u8) anyerror![]const u8 {
        scope_calls += 1;
        scoped_subject = oidc_subject;
        if (fail_scopes) return error.ProviderUnavailable;
        return alloc.dupe(u8, claim);
    }
};

const VALID_CREDENTIAL = "afc_" ++ "a" ** 64;

fn runWith(header: ?[]const u8) !struct { outcome: chain.Outcome, ctx: AuthCtx } {
    var host: usize = 0;
    var mw = CliCredential{
        .host = @ptrCast(&host),
        .lookup = test_lookup.lookup,
        .scope_host = @ptrCast(&host),
        .resolveScopes = test_lookup.resolveScopes,
    };

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    if (header) |h| ht.header("authorization", h);

    var ctx = AuthCtx{
        .alloc = testing.allocator,
        .res = ht.res,
        .req_id = "req_test",
        .write_error = test_lookup.writeError,
    };
    const outcome = try mw.execute(&ctx, ht.req);
    return .{ .outcome = outcome, .ctx = ctx };
}

test "a live credential resolves to the user who created it" {
    test_lookup.reset();
    test_lookup.row = .{
        .credential_id = "cred_1",
        .user_id = "01900000-0000-7000-8000-000000000001",
        .tenant_id = "tenant_1",
        .deployment = "https://api.agentsfleet.net",
        .revoked = false,
        .oidc_subject = "user_2abcXYZ",
    };
    test_lookup.claim = "fleet:read schedule:read";

    const result = try runWith("Bearer " ++ VALID_CREDENTIAL);
    defer if (result.ctx.principal) |p| {
        if (p.user_id) |v| testing.allocator.free(v);
        if (p.tenant_id) |v| testing.allocator.free(v);
    };

    try testing.expectEqual(chain.Outcome.next, result.outcome);
    const principal = result.ctx.principal.?;
    // Dimension 1.1 — a user principal, never a tenant-wide one.
    try testing.expectEqual(principal_mod.AuthMode.cli_credential, principal.mode);
    // `user_id` carries the identity provider's SUBJECT, not the row's UUID.
    // Every other path means the same thing by this field, and downstream code
    // resolves a user by `oidc_subject` — handing it the UUID would resolve
    // nothing, silently, on a route that looked authorised.
    try testing.expectEqualStrings("user_2abcXYZ", principal.user_id.?);
    try testing.expect(!std.mem.eql(u8, principal.user_id.?, "01900000-0000-7000-8000-000000000001"));
}

test "a revoked credential answers its own code, not a generic refusal" {
    test_lookup.reset();
    test_lookup.row = .{
        .credential_id = "cred_1",
        .user_id = "01900000-0000-7000-8000-000000000001",
        .tenant_id = "tenant_1",
        .deployment = "https://api.agentsfleet.net",
        .revoked = true,
        .oidc_subject = "user_2abcXYZ",
    };

    const result = try runWith("Bearer " ++ VALID_CREDENTIAL);

    // Dimension 1.5 — the operator is told to log in again, not left guessing.
    try testing.expectEqual(chain.Outcome.short_circuit, result.outcome);
    try testing.expectEqualStrings(ERR_CLI_CREDENTIAL_REVOKED, test_lookup.last_code);
}

test "an unknown credential is refused without naming what was wrong" {
    test_lookup.reset();
    const result = try runWith("Bearer " ++ VALID_CREDENTIAL);

    try testing.expectEqual(chain.Outcome.short_circuit, result.outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, test_lookup.last_code);
}

test "a malformed value never reaches the datastore" {
    test_lookup.reset();
    // A session token in the credential slot: right idea, wrong shape.
    const result = try runWith("Bearer header.payload.sig");

    try testing.expectEqual(chain.Outcome.short_circuit, result.outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, test_lookup.last_code);
    // The shape check runs before the lookup, so a bad paste costs no query.
    try testing.expectEqual(@as(usize, 0), test_lookup.call_count);
}

test "a lookup failure is unavailable, not unauthorized" {
    test_lookup.reset();
    test_lookup.fail_lookup = true;

    const result = try runWith("Bearer " ++ VALID_CREDENTIAL);

    // A datastore outage must not read as a bad credential — that would send
    // an operator to re-login over an incident they cannot fix.
    try testing.expectEqual(chain.Outcome.short_circuit, result.outcome);
    try testing.expectEqualStrings(errors.ERR_AUTH_UNAVAILABLE, test_lookup.last_code);
}

test "a missing Authorization header is refused before anything else" {
    test_lookup.reset();
    const result = try runWith(null);

    try testing.expectEqual(chain.Outcome.short_circuit, result.outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, test_lookup.last_code);
    try testing.expectEqual(@as(usize, 0), test_lookup.call_count);
}

fn liveRow() void {
    test_lookup.row = .{
        .credential_id = "cred_1",
        .user_id = "01900000-0000-7000-8000-000000000001",
        .tenant_id = "tenant_1",
        .deployment = "https://api.agentsfleet.net",
        .revoked = false,
        .oidc_subject = "user_2abcXYZ",
    };
}

test "capabilities come from the provider, keyed on the subject the row carries" {
    test_lookup.reset();
    liveRow();
    test_lookup.claim = "fleet:read schedule:read";

    const result = try runWith("Bearer " ++ VALID_CREDENTIAL);
    defer if (result.ctx.principal) |p| {
        if (p.user_id) |v| testing.allocator.free(v);
        if (p.tenant_id) |v| testing.allocator.free(v);
    };

    try testing.expectEqual(chain.Outcome.next, result.outcome);
    // Asked about this credential's owner, not some ambient identity.
    try testing.expectEqual(@as(usize, 1), test_lookup.scope_calls);
    try testing.expectEqualStrings("user_2abcXYZ", test_lookup.scoped_subject);
}

test "a narrowly provisioned person keeps their narrow capabilities" {
    // The regression this design exists to prevent: a read-only collaborator
    // must not be widened to a full grant by running `login`. A code-authored
    // grant would do exactly that; resolving the claim cannot.
    test_lookup.reset();
    liveRow();
    test_lookup.claim = "fleet:read";

    const result = try runWith("Bearer " ++ VALID_CREDENTIAL);
    defer if (result.ctx.principal) |p| {
        if (p.user_id) |v| testing.allocator.free(v);
        if (p.tenant_id) |v| testing.allocator.free(v);
    };

    const held = result.ctx.principal.?.scopes;
    try testing.expect(scopes.satisfiesAny(held, &.{.fleet_read}));
    try testing.expect(!scopes.satisfiesAny(held, &.{.fleet_admin}));
    try testing.expect(!scopes.satisfiesAny(held, &.{.approval_resolve}));
}

test "an unreachable provider is unavailable, never a silent empty grant" {
    // Failing open would hand the terminal no capabilities and read as
    // "forbidden" everywhere; failing closed with the wrong code would send an
    // operator to re-login over an outage they cannot fix. Neither: it is an
    // availability fault and says so.
    test_lookup.reset();
    liveRow();
    test_lookup.fail_scopes = true;

    const result = try runWith("Bearer " ++ VALID_CREDENTIAL);

    try testing.expectEqual(chain.Outcome.short_circuit, result.outcome);
    try testing.expectEqualStrings(errors.ERR_AUTH_UNAVAILABLE, test_lookup.last_code);
    try testing.expect(result.ctx.principal == null);
}

test "a revoked credential costs no provider call" {
    test_lookup.reset();
    test_lookup.row = .{
        .credential_id = "cred_1",
        .user_id = "01900000-0000-7000-8000-000000000001",
        .tenant_id = "tenant_1",
        .deployment = "https://api.agentsfleet.net",
        .revoked = true,
        .oidc_subject = "user_2abcXYZ",
    };

    _ = try runWith("Bearer " ++ VALID_CREDENTIAL);

    // Revocation is decided from the row alone — asking the provider about a
    // credential already known dead is a wasted round trip on a hot path.
    try testing.expectEqual(@as(usize, 0), test_lookup.scope_calls);
}
