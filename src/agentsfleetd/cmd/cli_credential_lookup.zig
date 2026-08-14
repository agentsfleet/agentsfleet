//! DB-backed `LookupFn` for the `cli_credential` middleware.
//!
//! `src/auth/middleware/` is portability-locked — it cannot reach into
//! `src/db/`. This module lives in `src/cmd/` (alongside the serve host that
//! wires it) and provides the concrete SHA-256-hex → `core.cli_credentials`
//! resolution, duplicating the kept fields into the caller's allocator.
//!
//! Sibling of `api_key_lookup.zig`, with two deliberate differences:
//!
//!   1. **No usage stamp.** `api_key_lookup` writes `last_used_at` on every
//!      authenticated request. `core.cli_credentials` has no such column and
//!      this path performs no write at all — attribution is recorded once, at
//!      mint, so the hottest indexed read in the system stays a read.
//!   2. **It joins `core.users`.** The row carries `user_id`, but the scope
//!      resolver needs the identity provider's subject to ask who this person
//!      is now. `oidc_subject` is that subject and it lives one join away, so
//!      the authenticate path fetches it in the same round trip rather than
//!      issuing a second query per request.

const std = @import("std");
const pg = @import("pg");

const db = @import("../db/pool.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const sql = @import("../state/sql.zig");
const cli_credential_mw = @import("../auth/middleware/cli_credential.zig");

pub const LookupResult = cli_credential_mw.LookupResult;

/// Host context carrying the shared connection pool. A stable pointer to a
/// value of this type is passed as `host` in the `LookupFn` call.
pub const Ctx = struct {
    pool: *pg.Pool,
};

/// Resolve a SHA-256 hex digest to a `core.cli_credentials` row joined to its
/// owning user. Returns null when no row matches. Allocates every returned
/// slice via `alloc`; the middleware frees what it does not adopt.
///
/// A revoked row is RETURNED, not filtered — the caller distinguishes "revoked"
/// from "never existed" so an operator whose credential was retired elsewhere
/// is told to log in again rather than left guessing.
pub fn lookup(
    host: *anyopaque,
    alloc: std.mem.Allocator,
    credential_hash_hex: []const u8,
) anyerror!?LookupResult {
    const self: *Ctx = @ptrCast(@alignCast(host));
    const conn = self.pool.acquire() catch return error.DbUnavailable;
    defer self.pool.release(conn);

    var q = PgQuery.from(conn.query(sql.SELECT_CLI_CREDENTIAL_BY_HASH, .{credential_hash_hex}) catch return error.DbQueryFailed);
    defer q.deinit();

    const row = (q.next() catch return error.DbQueryFailed) orelse return null;
    return try copyRow(alloc, row);
}

/// Takes `pg.Row` concretely rather than `anytype`: a generic row parameter is
/// only analysed once something instantiates it, which is how this module's
/// sibling store shipped with a column-shape mistake the compiler never saw.
fn copyRow(alloc: std.mem.Allocator, row: pg.Row) !LookupResult {
    const credential_id_raw = row.get([]u8, 0) catch return error.DbRowShape;
    const user_id_raw = row.get([]u8, 1) catch return error.DbRowShape;
    const tenant_id_raw = row.get([]u8, 2) catch return error.DbRowShape;
    const deployment_raw = row.get([]u8, 3) catch return error.DbRowShape;
    // `revoked_at` is nullable: present means retired, and the timestamp itself
    // is audit data this path does not need — only whether it is set.
    const revoked_at = row.get(?i64, 4) catch return error.DbRowShape;
    const oidc_subject_raw = row.get([]u8, 5) catch return error.DbRowShape;

    const credential_id = try alloc.dupe(u8, credential_id_raw);
    errdefer alloc.free(credential_id);
    const user_id = try alloc.dupe(u8, user_id_raw);
    errdefer alloc.free(user_id);
    const tenant_id = try alloc.dupe(u8, tenant_id_raw);
    errdefer alloc.free(tenant_id);
    const deployment = try alloc.dupe(u8, deployment_raw);
    errdefer alloc.free(deployment);
    const oidc_subject = try alloc.dupe(u8, oidc_subject_raw);

    return .{
        .credential_id = credential_id,
        .user_id = user_id,
        .tenant_id = tenant_id,
        .deployment = deployment,
        .revoked = revoked_at != null,
        .oidc_subject = oidc_subject,
    };
}

// ── Tests ────────────────────────────────────────────────────────────────

const testing = std.testing;

test {
    testing.refAllDecls(@This());
    _ = db;
}

test "the authenticate path issues no write" {
    // Dimension 1.6 — attribution is recorded at mint and never on this path.
    // `api_key_lookup` stamps `last_used_at` here; this table has no such
    // column and this statement must stay a pure read.
    const stmt = sql.SELECT_CLI_CREDENTIAL_BY_HASH;
    try testing.expect(std.mem.indexOf(u8, stmt, "UPDATE") == null);
    try testing.expect(std.mem.indexOf(u8, stmt, "INSERT") == null);
    try testing.expect(std.mem.indexOf(u8, stmt, "last_used_at") == null);
}

test "the lookup resolves the identity provider subject in one round trip" {
    // The scope resolver keys on `oidc_subject`. Fetching it in the same query
    // is what keeps the authenticate path at one round trip per request.
    const stmt = sql.SELECT_CLI_CREDENTIAL_BY_HASH;
    try testing.expect(std.mem.indexOf(u8, stmt, "u.oidc_subject") != null);
    try testing.expect(std.mem.indexOf(u8, stmt, "JOIN core.users") != null);
}

test "a revoked row is selected, not filtered out" {
    // Filtering on `revoked_at IS NULL` here would make a revoked credential
    // indistinguishable from an unknown one, and the operator would be told
    // the wrong thing.
    const stmt = sql.SELECT_CLI_CREDENTIAL_BY_HASH;
    try testing.expect(std.mem.indexOf(u8, stmt, "revoked_at IS NULL") == null);
    try testing.expect(std.mem.indexOf(u8, stmt, "c.revoked_at") != null);
}
