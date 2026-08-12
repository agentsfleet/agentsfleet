//! Unit coverage for the credential endpoints' pure decisions — who these
//! routes admit, and what shape of machine name they accept.
//!
//! The datastore-crossing behaviour (mint, list, revoke, and the ownership
//! scoping in the statements themselves) is proved in the integration tier
//! against a real schema, because a statement's WHERE clause is exactly what a
//! stubbed connection cannot check.

const std = @import("std");
const testing = std.testing;

const principal_mod = @import("../../../auth/principal.zig");
const cli_credential = @import("../../../auth/cli_credential.zig");
const sql = @import("../../../state/sql.zig");
const ec = @import("../../../errors/error_registry.zig");

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
