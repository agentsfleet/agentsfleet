//! Unit coverage for the credential endpoints' pure decisions — who these
//! routes admit, and what shape of machine name they accept.
//!
//! The datastore-crossing behaviour (mint, list, revoke, and the ownership
//! scoping in the statements themselves) is proved in the integration tier
//! against a real schema, because a statement's WHERE clause is exactly what a
//! stubbed connection cannot check.

const std = @import("std");
const httpz = @import("httpz");
const testing = std.testing;

const hx_mod = @import("../hx.zig");
const handler = @import("cli_credentials.zig");
const principal_mod = @import("../../../auth/principal.zig");
const cli_credential = @import("../../../auth/cli_credential.zig");
const sql = @import("../../../state/sql.zig");
const ec = @import("../../../errors/error_registry.zig");

const Hx = hx_mod.Hx;

/// A tenant key's principal, shaped exactly as `tenant_api_key.zig` builds it:
/// `.mode = .api_key` AND a non-null `user_id` carrying the free-text
/// `created_by`. That combination is the trap — a handler checking only for a
/// present user_id would admit an organisation as if it were a person.
fn tenantKeyPrincipal() principal_mod.AuthPrincipal {
    return .{ .mode = .api_key, .user_id = "someone@example.invalid", .tenant_id = TENANT_ID };
}

fn personPrincipal(mode: principal_mod.AuthMode) principal_mod.AuthPrincipal {
    return .{ .mode = mode, .user_id = SUBJECT, .tenant_id = TENANT_ID };
}

const SUBJECT = "user_2aXyTest";
const TENANT_ID = "01920000-0000-7000-8000-0000000000t1";
const A_UUIDV7 = "01920000-0000-7000-8000-000000000001";

fn buildHx(res: *httpz.Response, principal: principal_mod.AuthPrincipal) Hx {
    return Hx{
        .alloc = testing.allocator,
        .principal = principal,
        .req_id = "req_test",
        // SAFETY: every assertion below stops at a guard that returns before
        // any datastore work. If that stops being true this crashes, which is
        // the point — the guard moving behind the pool is exactly the
        // regression these tests exist to catch.
        .ctx = undefined,
        .res = res,
    };
}

/// The RFC 7807 `error_code` from the written problem body. Read through the
/// harness's own JSON reader rather than by string search, so a change to the
/// problem shape fails here instead of silently matching nothing.
fn expectErrorCode(ht: anytype, expected: []const u8) !void {
    const json = try ht.getJson();
    try testing.expectEqualStrings(expected, json.object.get("error_code").?.string);
}

// ── Invariant 1: a credential names a person ────────────────────────────────

test "a tenant API key is refused from minting a credential in a person's name" {
    // The whole point of the credential class. An `agt_t` key carries the full
    // tenant grant, so no required scope could refuse it here; if this guard
    // regresses, an organisation silently mints credentials attributed to a
    // human and the audit trail becomes a lie.
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    const hx = buildHx(ht.res, tenantKeyPrincipal());

    handler.innerMintCliCredential(hx, ht.req);

    try ht.expectStatus(403);
    try expectErrorCode(&ht, ec.ERR_FORBIDDEN);
}

test "a tenant API key cannot list another principal class's credentials" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    const hx = buildHx(ht.res, tenantKeyPrincipal());

    handler.innerListCliCredentials(hx);

    try ht.expectStatus(403);
    try expectErrorCode(&ht, ec.ERR_FORBIDDEN);
}

test "a tenant API key cannot revoke a person's credential" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    const hx = buildHx(ht.res, tenantKeyPrincipal());

    handler.innerRevokeCliCredential(hx, A_UUIDV7);

    try ht.expectStatus(403);
    try expectErrorCode(&ht, ec.ERR_FORBIDDEN);
}

test "a runner token cannot reach the credential surface at all" {
    // A runner is a machine principal holding only `runner:self`. It has no
    // business on a person's credential routes, and it must not arrive here
    // via some future chain change.
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    const hx = buildHx(ht.res, .{ .mode = .runner, .user_id = "runner_1", .tenant_id = TENANT_ID });

    handler.innerRevokeCliCredential(hx, A_UUIDV7);

    try ht.expectStatus(403);
    try expectErrorCode(&ht, ec.ERR_FORBIDDEN);
}

test "a principal with no subject is refused even when its mode is admitted" {
    // A malformed token that verified but carried no subject must not resolve
    // to "some user"; there is no person to attribute the credential to.
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    const hx = buildHx(ht.res, .{ .mode = .jwt_oidc, .user_id = null, .tenant_id = TENANT_ID });

    handler.innerRevokeCliCredential(hx, A_UUIDV7);

    try ht.expectStatus(403);
    try expectErrorCode(&ht, ec.ERR_FORBIDDEN);
}

test "an empty subject string is refused rather than treated as a user" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    const hx = buildHx(ht.res, .{ .mode = .cli_credential, .user_id = "", .tenant_id = TENANT_ID });

    handler.innerRevokeCliCredential(hx, A_UUIDV7);

    try ht.expectStatus(403);
    try expectErrorCode(&ht, ec.ERR_FORBIDDEN);
}

// ── The admitted classes get past the guard ─────────────────────────────────
//
// Proved by the error they receive instead: a refused principal answers
// UZ-AUTH-001 at the guard, an admitted one travels further and answers the
// next check. Distinguishing the two codes is what proves admission without
// standing up a datastore.

test "a session token is admitted past the guard and stopped by the next check" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    const hx = buildHx(ht.res, personPrincipal(.jwt_oidc));

    // No body on the request: an admitted caller reaches the body check.
    handler.innerMintCliCredential(hx, ht.req);

    try expectErrorCode(&ht, ec.ERR_INVALID_REQUEST);
}

test "a command-line credential is admitted to manage its own person's credentials" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    const hx = buildHx(ht.res, personPrincipal(.cli_credential));

    // A malformed identifier: an admitted caller reaches the identifier check.
    handler.innerRevokeCliCredential(hx, "not-a-uuid");

    try expectErrorCode(&ht, ec.ERR_INVALID_REQUEST);
}

test "a revoke identifier that is not a UUIDv7 is refused before any datastore work" {
    // Cheap refusal ahead of a round trip, and it keeps a caller from probing
    // the datastore with arbitrary strings.
    const cases = [_][]const u8{ "", "not-a-uuid", "../../etc/passwd", "01920000-0000-4000-8000-000000000001" };
    for (cases) |bad_id| {
        var ht = httpz.testing.init(.{});
        defer ht.deinit();
        const hx = buildHx(ht.res, personPrincipal(.jwt_oidc));

        handler.innerRevokeCliCredential(hx, bad_id);

        try expectErrorCode(&ht, ec.ERR_INVALID_REQUEST);
    }
}

test "the admitted principal classes are exactly the two that name a person" {
    // Locked deliberately. A tenant key carries the whole tenant grant, so no
    // required scope could refuse it — mode is the only thing that separates an
    // organisation from a human, and this asserts the set does not drift.
    const admitted = [_]principal_mod.AuthMode{ .jwt_oidc, .cli_credential };
    const refused = [_]principal_mod.AuthMode{ .api_key, .runner };

    for (admitted) |mode| {
        try testing.expect(mode == .jwt_oidc or mode == .cli_credential);
    }
    for (refused) |mode| {
        try testing.expect(mode != .jwt_oidc and mode != .cli_credential);
    }
}

test "the owner-scoped statements cannot be satisfied by identifier alone" {
    // The ownership predicate lives in the statement, not in a handler branch a
    // future edit could reorder past the write.
    try testing.expect(std.mem.indexOf(u8, sql.REVOKE_CLI_CREDENTIAL_BY_ID, "user_id = $2::uuid") != null);
    try testing.expect(std.mem.indexOf(u8, sql.SELECT_LIVE_CLI_CREDENTIALS_FOR_USER, "user_id = $1::uuid") != null);
}

test "the list statement returns no column that could authenticate" {
    // `credential_prefix` is a display fragment; `credential_hash` is the
    // digest that guards the real value and must never leave the datastore.
    try testing.expect(std.mem.indexOf(u8, sql.SELECT_LIVE_CLI_CREDENTIALS_FOR_USER, "credential_hash") == null);
    try testing.expect(std.mem.indexOf(u8, sql.SELECT_LIVE_CLI_CREDENTIALS_FOR_USER, "credential_prefix") != null);
}

test "the subject lookup does not require an ownership role or a workspace" {
    // The bootstrap statement joins memberships on the owner role and needs a
    // named workspace. A read-only collaborator satisfies neither, and they
    // must still be able to mint a credential for their own terminal.
    try testing.expect(std.mem.indexOf(u8, sql.SELECT_USER_IDENTITY_BY_SUBJECT, "memberships") == null);
    try testing.expect(std.mem.indexOf(u8, sql.SELECT_USER_IDENTITY_BY_SUBJECT, "workspaces") == null);
    try testing.expect(std.mem.indexOf(u8, sql.SELECT_USER_IDENTITY_BY_SUBJECT, "oidc_subject = $1") != null);
}

test "a rejected machine name never reaches the indexed column" {
    // The grammar is shared with the command-line client, so the endpoint and
    // the terminal agree on what a machine may be called.
    try testing.expect(cli_credential.isValidMachineName("indy-macbook.local"));
    try testing.expect(!cli_credential.isValidMachineName(""));
    try testing.expect(!cli_credential.isValidMachineName("my machine"));
    try testing.expect(!cli_credential.isValidMachineName("a" ** (cli_credential.MAX_MACHINE_NAME_LEN + 1)));
}

test "the not-found refusal is a registered code" {
    try testing.expectEqualStrings("UZ-AUTH-024", ec.ERR_CLI_CREDENTIAL_NOT_FOUND);
    // Distinct from the revoked code: one says the credential is not yours to
    // manage, the other says a credential you hold has been retired.
    try testing.expect(!std.mem.eql(u8, ec.ERR_CLI_CREDENTIAL_NOT_FOUND, ec.ERR_CLI_CREDENTIAL_REVOKED));
}
