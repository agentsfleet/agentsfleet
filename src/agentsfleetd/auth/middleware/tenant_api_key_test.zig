//! Behavioural tests for the `tenant_api_key` middleware.
//!
//! Split from the module under test to hold it under the file-length limit,
//! matching the `*_test.zig` siblings already in this directory. Everything
//! here drives the public surface — `TenantApiKey`, `LookupResult`, and the
//! two injected callbacks — so the middleware needs neither a datastore nor
//! the identity provider to be provable.

const std = @import("std");
const httpz = @import("httpz");

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const errors = @import("errors.zig");
const principal_mod = @import("../principal.zig");
const mw_mod = @import("tenant_api_key.zig");

const AuthCtx = auth_ctx.AuthCtx;
const TenantApiKey = mw_mod.TenantApiKey;
const LookupResult = mw_mod.LookupResult;
const TENANT_KEY_PREFIX = mw_mod.TENANT_KEY_PREFIX;

const auth_codes = @import("auth_codes");
const ERR_APIKEY_REVOKED = auth_codes.ERR_APIKEY_REVOKED;

const testing = std.testing;

const MockLookup = struct {
    want_hash: []const u8 = "",
    return_row: ?LookupResult = null,
    return_err: ?anyerror = null,
    called_with: []const u8 = "",
    call_count: usize = 0,

    fn fn_(host: *anyopaque, alloc: std.mem.Allocator, key_hash_hex: []const u8) anyerror!?LookupResult {
        const self: *MockLookup = @ptrCast(@alignCast(host));
        self.called_with = key_hash_hex;
        self.call_count += 1;
        if (self.return_err) |e| return e;
        if (self.return_row) |row| {
            return .{
                .api_key_id = try alloc.dupe(u8, row.api_key_id),
                .tenant_id = try alloc.dupe(u8, row.tenant_id),
                .user_id = try alloc.dupe(u8, row.user_id),
                .active = row.active,
            };
        }
        return null;
    }
};

/// Stands in for the identity provider. Records the subject it was asked
/// about, so a test can prove the middleware resolves against `created_by`
/// rather than any other field on the row.
const MockScopes = struct {
    claim: []const u8 = "fleet:admin schedule:write",
    return_err: ?anyerror = null,
    asked_about: []const u8 = "",
    call_count: usize = 0,

    fn fn_(scope_host: *anyopaque, alloc: std.mem.Allocator, oidc_subject: []const u8) anyerror![]const u8 {
        const self: *MockScopes = @ptrCast(@alignCast(scope_host));
        self.asked_about = oidc_subject;
        self.call_count += 1;
        if (self.return_err) |e| return e;
        return alloc.dupe(u8, self.claim);
    }
};

/// The middleware under test, wired to both stubs. A helper rather than a
/// literal at each call site: the struct now takes four fields, and a test that
/// spelled them out would break on every future field for no reason of its own.
fn mockMw(lookup: *MockLookup, scope_stub: *MockScopes) TenantApiKey {
    return .{
        .host = lookup,
        .lookup = MockLookup.fn_,
        .scope_host = scope_stub,
        .resolveScopes = MockScopes.fn_,
    };
}

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

fn makeCtx(res: *httpz.Response) AuthCtx {
    return .{
        .alloc = testing.allocator,
        .res = res,
        .req_id = "req_test",
        .write_error = test_fixtures.writeError,
    };
}

test "tenant_api_key rejects missing Authorization header with UZ-AUTH-002" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    var mock = MockLookup{};
    var scope_stub = MockScopes{};
    var mw = mockMw(&mock, &scope_stub);
    var ctx = makeCtx(ht.res);
    const outcome = try mw.execute(&ctx, ht.req);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqual(@as(usize, 1), test_fixtures.write_count);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, test_fixtures.last_code);
    try testing.expectEqual(@as(usize, 0), mock.call_count);
    try testing.expect(ctx.principal == null);
}

test "tenant_api_key rejects Bearer token without agt_t prefix without calling lookup" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer agt_anotatenantkey");

    var mock = MockLookup{};
    var scope_stub = MockScopes{};
    var mw = mockMw(&mock, &scope_stub);
    var ctx = makeCtx(ht.res);
    const outcome = try mw.execute(&ctx, ht.req);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, test_fixtures.last_code);
    try testing.expectEqual(@as(usize, 0), mock.call_count);
    try testing.expect(ctx.principal == null);
}

test "tenant_api_key rejects unknown key with UZ-AUTH-002 and emits rejected log" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer agt_t" ++ "0" ** 64);

    var mock = MockLookup{ .return_row = null };
    var scope_stub = MockScopes{};
    var mw = mockMw(&mock, &scope_stub);
    var ctx = makeCtx(ht.res);
    const outcome = try mw.execute(&ctx, ht.req);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, test_fixtures.last_code);
    try testing.expectEqual(@as(usize, 1), mock.call_count);
    try testing.expect(ctx.principal == null);
}

test "tenant_api_key rejects revoked key with UZ-APIKEY-004 and frees row slices" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer agt_t" ++ "a" ** 64);

    var mock = MockLookup{
        .return_row = .{
            .api_key_id = "11111111-1111-7111-8111-111111111111",
            .tenant_id = "22222222-2222-7222-8222-222222222222",
            .user_id = "33333333-3333-7333-8333-333333333333",
            .active = false,
        },
    };
    var scope_stub = MockScopes{};
    var mw = mockMw(&mock, &scope_stub);
    var ctx = makeCtx(ht.res);
    const outcome = try mw.execute(&ctx, ht.req);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(ERR_APIKEY_REVOKED, test_fixtures.last_code);
    try testing.expect(ctx.principal == null);
}

test "tenant_api_key populates principal on active key match" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer agt_t" ++ "b" ** 64);

    var mock = MockLookup{
        .return_row = .{
            .api_key_id = "11111111-1111-7111-8111-111111111111",
            .tenant_id = "22222222-2222-7222-8222-222222222222",
            .user_id = "33333333-3333-7333-8333-333333333333",
            .active = true,
        },
    };
    var scope_stub = MockScopes{};
    var mw = mockMw(&mock, &scope_stub);
    var ctx = makeCtx(ht.res);
    const outcome = try mw.execute(&ctx, ht.req);
    defer if (ctx.principal) |p| {
        if (p.user_id) |v| testing.allocator.free(v);
        if (p.tenant_id) |v| testing.allocator.free(v);
    };

    try testing.expectEqual(chain.Outcome.next, outcome);
    try testing.expectEqual(@as(usize, 0), test_fixtures.write_count);
    try testing.expect(ctx.principal != null);
    try testing.expectEqual(principal_mod.AuthMode.api_key, ctx.principal.?.mode);
    // The stub answered "fleet:admin schedule:write", and that is what the
    // principal holds — nothing here widened it to a bundle.
    try testing.expect(ctx.principal.?.scopes.contains(.fleet_admin));
    try testing.expect(ctx.principal.?.scopes.contains(.schedule_write));
    try testing.expect(!ctx.principal.?.scopes.contains(.runner_enroll));
    try testing.expectEqualStrings("33333333-3333-7333-8333-333333333333", ctx.principal.?.user_id.?);
    try testing.expectEqualStrings("22222222-2222-7222-8222-222222222222", ctx.principal.?.tenant_id.?);
}

test "test_tenant_key_scopes_come_from_clerk" {
    // Dimension 6.1. Two things at once, because either alone would pass while
    // the other was wrong: the resolver is asked about the subject in
    // `created_by`, and the answer is what lands on the principal.
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer agt_t" ++ "d" ** 64);

    const CREATOR_SUBJECT = "user_2creatorOfThisKey";
    var mock = MockLookup{
        .return_row = .{
            .api_key_id = "11111111-1111-7111-8111-111111111111",
            .tenant_id = "22222222-2222-7222-8222-222222222222",
            .user_id = CREATOR_SUBJECT,
            .active = true,
        },
    };
    // Deliberately NOT the retired bundle: a set no compiled-in grant ever
    // produced, so a principal carrying it can only have come from the provider.
    var scope_stub = MockScopes{ .claim = "billing:read" };
    var mw = mockMw(&mock, &scope_stub);
    var ctx = makeCtx(ht.res);
    const outcome = try mw.execute(&ctx, ht.req);
    defer if (ctx.principal) |p| {
        if (p.user_id) |v| testing.allocator.free(v);
        if (p.tenant_id) |v| testing.allocator.free(v);
    };

    try testing.expectEqual(chain.Outcome.next, outcome);
    try testing.expectEqual(@as(usize, 1), scope_stub.call_count);
    try testing.expectEqualStrings(CREATOR_SUBJECT, scope_stub.asked_about);
    try testing.expect(ctx.principal.?.scopes.contains(.billing_read));
    // The capabilities the retired grant would have handed out, absent now that
    // nothing hands out anything.
    try testing.expect(!ctx.principal.?.scopes.contains(.fleet_admin));
    try testing.expect(!ctx.principal.?.scopes.contains(.workspace_admin));
}

test "test_creator_approval_capability_reaches_the_key" {
    // Dimension 6.5. The reversal, pinned: the old machine grant subtracted
    // `approval:resolve` so a Fleet holding a key could not approve its own
    // gate. Inheritance is now exact, so a creator who holds it mints a key
    // that holds it. Asserted rather than left implicit — this is a decision,
    // and a future reader must find it stated somewhere that fails when broken.
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer agt_t" ++ "e" ** 64);

    var mock = MockLookup{
        .return_row = .{
            .api_key_id = "11111111-1111-7111-8111-111111111111",
            .tenant_id = "22222222-2222-7222-8222-222222222222",
            .user_id = "user_2ownerWhoApproves",
            .active = true,
        },
    };
    var scope_stub = MockScopes{ .claim = "approval:resolve" };
    var mw = mockMw(&mock, &scope_stub);
    var ctx = makeCtx(ht.res);
    const outcome = try mw.execute(&ctx, ht.req);
    defer if (ctx.principal) |p| {
        if (p.user_id) |v| testing.allocator.free(v);
        if (p.tenant_id) |v| testing.allocator.free(v);
    };

    try testing.expectEqual(chain.Outcome.next, outcome);
    try testing.expect(ctx.principal.?.scopes.contains(.approval_resolve));
    // And the ladder's lower rung comes with it, so the key reaches the whole
    // approval surface rather than a half of it.
    try testing.expect(ctx.principal.?.scopes.contains(.approval_read));
}

test "test_tenant_key_provider_outage_is_unavailable" {
    // Dimension 6.4. The failure that must NOT be an empty grant: a key whose
    // creator cannot be resolved is refused as unavailable, so an outage never
    // silently downgrades a working automation into one that authenticates and
    // then fails every gate — two different incidents with two different fixes.
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer agt_t" ++ "f" ** 64);

    var mock = MockLookup{
        .return_row = .{
            .api_key_id = "11111111-1111-7111-8111-111111111111",
            .tenant_id = "22222222-2222-7222-8222-222222222222",
            .user_id = "user_2unreachable",
            .active = true,
        },
    };
    var scope_stub = MockScopes{ .return_err = error.Unexpected };
    var mw = mockMw(&mock, &scope_stub);
    var ctx = makeCtx(ht.res);
    const outcome = try mw.execute(&ctx, ht.req);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_AUTH_UNAVAILABLE, test_fixtures.last_code);
    try testing.expect(ctx.principal == null);
}

test "test_unknown_creator_yields_no_capability" {
    // Dimension 6.3. The resolver answers a deleted creator with an EMPTY
    // claim, not an error (a deletion is permanent; retrying cannot help), and
    // the middleware must pass that emptiness through: the key authenticates,
    // holds nothing, and every capability gate refuses it by scope. The
    // distinction from 6.4 is the whole point — unknown-creator is a fact,
    // outage is a fault, and only the fault says "try again".
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer agt_t" ++ "1" ** 64);

    var mock = MockLookup{
        .return_row = .{
            .api_key_id = "11111111-1111-7111-8111-111111111111",
            .tenant_id = "22222222-2222-7222-8222-222222222222",
            .user_id = "user_2deletedAtProvider",
            .active = true,
        },
    };
    var scope_stub = MockScopes{ .claim = "" };
    var mw = mockMw(&mock, &scope_stub);
    var ctx = makeCtx(ht.res);
    const outcome = try mw.execute(&ctx, ht.req);
    defer if (ctx.principal) |p| {
        if (p.user_id) |v| testing.allocator.free(v);
        if (p.tenant_id) |v| testing.allocator.free(v);
    };

    try testing.expectEqual(chain.Outcome.next, outcome);
    try testing.expectEqual(@as(usize, 0), test_fixtures.write_count);
    try testing.expectEqual(@as(usize, 0), ctx.principal.?.scopes.count());
}

test "tenant_api_key surfaces LookupFn error as UZ-AUTH-004" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer agt_t" ++ "c" ** 64);

    var mock = MockLookup{ .return_err = error.Unexpected };
    var scope_stub = MockScopes{};
    var mw = mockMw(&mock, &scope_stub);
    var ctx = makeCtx(ht.res);
    const outcome = try mw.execute(&ctx, ht.req);

    try testing.expectEqual(chain.Outcome.short_circuit, outcome);
    try testing.expectEqualStrings(errors.ERR_AUTH_UNAVAILABLE, test_fixtures.last_code);
    try testing.expect(ctx.principal == null);
}

test "TENANT_KEY_PREFIX is the documented agt_t literal" {
    try testing.expectEqualStrings("agt_t", TENANT_KEY_PREFIX);
}
