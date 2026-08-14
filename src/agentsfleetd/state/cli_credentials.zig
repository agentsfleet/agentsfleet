//! Store for `core.cli_credentials` — the durable, user-scoped credential
//! `agentsfleet login` mints. Handler-side only: mint, list, and revoke.
//!
//! The authenticate-path lookup is deliberately NOT here. `src/auth/` is
//! portability-locked and cannot reach into the datastore, so the digest → row
//! resolution lives in `cmd/cli_credential_lookup.zig` as an injected callback,
//! exactly as `cmd/api_key_lookup.zig` does for tenant keys.
//!
//! `credential_hash` is derived, never accepted. `mint` is the only writer, and
//! it computes the digest from the value it just generated — there is no path
//! that takes a hash from a caller. If a client could supply a digest, that
//! digest would BE the credential and storing a hash would protect nothing.

const std = @import("std");
const pg = @import("pg");
const logging = @import("log");
const clock = @import("common").clock;
const sql = @import("sql.zig");
const cli_credential = @import("../auth/cli_credential.zig");
const api_key = @import("../auth/api_key.zig");
const id_format = @import("../types/id_format.zig");

const log = logging.scoped(.cli_credentials);

const S_BEGIN = "BEGIN";
const S_COMMIT = "COMMIT";

/// What a freshly minted credential hands back. `secret` is the only time the
/// raw value exists outside the caller's process — it is returned once, stored
/// by the terminal, and never retrievable again. The caller owns both slices.
pub const Minted = struct {
    id: []const u8,
    secret: []const u8,

    pub fn deinit(self: Minted, alloc: std.mem.Allocator) void {
        alloc.free(self.id);
        alloc.free(self.secret);
    }
};

/// Everything the mint needs from its caller. The digest is absent on purpose:
/// it is derived here from the value generated here.
pub const NewCredential = struct {
    user_id: []const u8,
    tenant_id: []const u8,
    machine_name: []const u8,
    deployment: []const u8,
    created_from_address: []const u8,
};

/// Revoke this machine's live credential, then mint its replacement — as one
/// transaction.
///
/// The two steps are ordered, not optional: the partial unique index on
/// (user_id, machine_name) WHERE revoked_at IS NULL refuses a second live row,
/// so skipping the revoke turns a re-login into a loud insert failure rather
/// than two live credentials an operator cannot tell apart.
///
/// They are also atomic, which is what keeps a FAILED mint from destroying a
/// working terminal: the revoke commits only if the insert does, so an
/// operator whose re-login fails still holds the credential they arrived with.
/// This function owns that transaction rather than asking callers for it — the
/// previous shape declared the requirement here in prose and the only caller
/// did not honour it, which is what a precondition written in a comment
/// invites.
pub fn mint(alloc: std.mem.Allocator, conn: *pg.Conn, new: NewCredential) !Minted {
    // Both are generated before the transaction opens. Neither touches the
    // datastore, and holding a transaction open across them would widen the
    // window on this write path for nothing.
    const secret = try cli_credential.generate(alloc);
    errdefer alloc.free(secret);

    const row_id = try id_format.generateCliCredentialId(alloc);
    errdefer alloc.free(row_id);

    const digest = api_key.sha256Hex(secret);

    _ = try conn.exec(S_BEGIN, .{});
    // Registered BEFORE the first statement inside the transaction so a failure
    // of ANY of them rolls back; an errdefer placed later would strand an open
    // transaction on the pooled connection. `conn.rollback()` rather than
    // `exec("ROLLBACK")` because the driver's exec short-circuits once the
    // connection is in FAIL state, which would leave the session stuck in an
    // aborted transaction (`account_teardown.zig`, `signup_bootstrap.zig`).
    errdefer conn.rollback() catch |err|
        log.warn(logging.EVENT_IGNORED_ERROR, .{ .err = @errorName(err) });

    _ = try conn.exec(sql.LOCK_CLI_CREDENTIAL_MINT, .{ new.user_id, new.machine_name });
    _ = try revokeForMachine(conn, new.user_id, new.machine_name);
    _ = try conn.exec(sql.INSERT_CLI_CREDENTIAL, .{
        row_id,
        new.user_id,
        new.tenant_id,
        new.machine_name,
        &digest,
        cli_credential.displayPrefix(secret),
        new.deployment,
        new.created_from_address,
        clock.nowMillis(),
    });
    _ = try conn.exec(S_COMMIT, .{});

    return .{ .id = row_id, .secret = secret };
}

/// Revoke every live credential for one (user, machine). Returns how many rows
/// were revoked — zero is a first login, not an error. Scoped to one machine so
/// a re-login on this laptop leaves another machine's terminal working.
pub fn revokeForMachine(
    conn: *pg.Conn,
    user_id: []const u8,
    machine_name: []const u8,
) !usize {
    const affected = try conn.exec(sql.REVOKE_CLI_CREDENTIAL_FOR_MACHINE, .{
        user_id, machine_name, clock.nowMillis(),
    });
    return @intCast(affected orelse 0);
}

/// Revoke one credential by identifier, scoped to its owner. Returns whether a
/// row was revoked; a credential belonging to somebody else is indistinguishable
/// from one that does not exist, so an identifier cannot be probed for existence.
pub fn revokeById(
    conn: *pg.Conn,
    credential_id: []const u8,
    user_id: []const u8,
) !bool {
    const affected = try conn.exec(sql.REVOKE_CLI_CREDENTIAL_BY_ID, .{
        credential_id, user_id, clock.nowMillis(),
    });
    return (affected orelse 0) > 0;
}

// ── Tests ────────────────────────────────────────────────────────────────

const testing = std.testing;

// Forces semantic analysis of every declaration above.
//
// Zig only analyses a function body once something references it, so a helper
// orphaned by a future refactor still has to compile rather than rotting
// silently.
test {
    testing.refAllDecls(@This());
}

test "both revoke statements are owner-scoped" {
    // A revoke that forgot `user_id` would let any caller retire a credential
    // by guessing its identifier, and would make re-login revoke every machine.
    try testing.expect(std.mem.indexOf(u8, sql.REVOKE_CLI_CREDENTIAL_BY_ID, "user_id") != null);
    try testing.expect(std.mem.indexOf(u8, sql.REVOKE_CLI_CREDENTIAL_FOR_MACHINE, "user_id") != null);
    try testing.expect(std.mem.indexOf(u8, sql.REVOKE_CLI_CREDENTIAL_FOR_MACHINE, "machine_name") != null);
}

test "revoking only ever touches live rows" {
    // `revoked_at IS NULL` keeps a re-revoke from overwriting the original
    // timestamp, so the audit trail records when a credential actually died.
    try testing.expect(std.mem.indexOf(u8, sql.REVOKE_CLI_CREDENTIAL_BY_ID, "revoked_at IS NULL") != null);
    try testing.expect(std.mem.indexOf(u8, sql.REVOKE_CLI_CREDENTIAL_FOR_MACHINE, "revoked_at IS NULL") != null);
}
