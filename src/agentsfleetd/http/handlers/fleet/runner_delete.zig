//! DELETE /v1/fleets/runners/{id} — retire a revoked runner's record.
//!
//! Mirrors the API-key revoke-then-delete lifecycle (handlers/api_keys/tenant.zig
//! innerDeleteApiKey): only an already-revoked row is deletable, so the
//! destructive step stays PATCH `revoke` and delete merely retires the record.
//! Scope is runner:write, the same as revoke — deleting a dead row is strictly
//! less consequential than taking a live runner out of service.
//!
//! NOT tenant-scoped. fleet.runners.tenant_id is NULL for the trusted fleet, so
//! a tenant predicate here would match zero rows; authorization is by scope
//! alone, exactly as runner_patch.zig does.
//!
//! Cascade side effects, all declared in schema and intentional:
//!   - fleet.runner_leases   ON DELETE CASCADE   (018)
//!   - fleet.runner_events   ON DELETE CASCADE   (021)
//!   - fleet.runner_affinity ON DELETE SET NULL  (019) — drops fleet-to-runner
//!     stickiness, so the next assign re-picks freely.
//! Deleting lease rows is safe: mutual exclusion lives in
//! fleet.runner_affinity.fencing_seq, never in the lease row, and every fence
//! check drives off the lease row via INNER JOIN, so a missing lease row is
//! uniformly a rejection rather than a bypass.

const pg = @import("pg");
const sql = @import("sql.zig");
const logging = @import("log");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const protocol = @import("contract").protocol;

const Hx = hx_mod.Hx;
const log = logging.scoped(.fleet_runner_delete);

const S_RUNNER_NOT_FOUND = "Runner not found";
const S_MUST_REVOKE_FIRST = "Active runner must be revoked before deletion";

pub fn innerDeleteFleetRunner(hx: Hx, runner_id: []const u8) void {
    if (!common.requireUuidV7Id(hx.res, hx.req_id, runner_id, "runner_id")) return;

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const outcome = deleteRevoked(conn, runner_id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    switch (outcome) {
        .missing => hx.fail(ec.ERR_RUNNER_NOT_FOUND, S_RUNNER_NOT_FOUND),
        .not_revoked => hx.fail(ec.ERR_RUNNER_MUST_REVOKE_FIRST, S_MUST_REVOKE_FIRST),
        .deleted => {
            log.debug("runner_deleted", .{ .runner_id = runner_id });
            hx.noContent();
        },
    }
}

const Outcome = enum { missing, not_revoked, deleted };

/// One round-trip that distinguishes "no such runner" from "present but still
/// active" — the same CTE idiom as innerDeleteApiKey. Splitting this into a
/// SELECT-then-DELETE would race an operator revoking concurrently.
fn deleteRevoked(conn: *pg.Conn, runner_id: []const u8) !Outcome {
    var q = PgQuery.from(conn.query(sql.DELETE_RUNNER_IF_IN_STATE, .{ runner_id, @tagName(protocol.AdminState.revoked) }) catch return error.DbError);
    defer q.deinit();

    const row = q.next() catch return error.DbError;
    if (row == null) return .missing;
    const changed = row.?.get(bool, 1) catch return error.DbError;
    return if (changed) .deleted else .not_revoked;
}

test {
    _ = @import("runner_delete_test.zig");
}
