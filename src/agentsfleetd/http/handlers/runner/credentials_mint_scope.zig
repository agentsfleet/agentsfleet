//! Lease-scope resolution for the on-demand credential mint — split from
//! `credentials_mint.zig` for the file-length budget (RULE FLL).
//!
//! Owns the one query that answers "what may this lease mint for, and how far":
//! the lease's workspace and fleet (Invariant 2 — the runner-id scope IS the
//! ownership check) plus the fleet's repository egress binding, all from a single
//! row so the binding can never be resolved from a different fleet than the one
//! the lease authorized.

const pg = @import("pg");

const sql = @import("sql.zig");
const hx_mod = @import("../hx.zig");
const constants = @import("common");
const ec = @import("../../../errors/error_registry.zig");
const pg_query = @import("../../../db/pg_query.zig");
const integration = @import("../../../credentials/integration.zig");
const fleet_config = @import("../../../fleet_runtime/config.zig");
const logging = @import("log");
const protocol = @import("contract").protocol;

const Hx = hx_mod.Hx;
const PgQuery = pg_query.PgQuery;
const log = logging.scoped(.credential_mint);

/// The lease's workspace + fleet, both arena-duped (survive the conn release),
/// plus the fleet's repository egress binding read from the same row.
pub const LeaseScope = struct {
    workspace_id: []const u8,
    fleet_id: []const u8,
    repository_binding: ?integration.RepositoryBinding,
};

/// Resolve the lease scoped to the presenting runner (Invariant 2: the runner-id
/// scope is the ownership check) AND still live — `status = active` and unexpired.
/// A foreign, expired, or revoked `lease_id` → null → 404, so mint authority is
/// bound to the lease's lifetime, not the runner's: a cancelled/expired run, or a
/// compromised runner replaying a stale `lease_id`, cannot mint past the lease.
/// Mirrors the active-lease predicate the sibling `memory.zig` already enforces.
/// Also returns the lease's fleet id — the scope the grant-gate checks (the grant gate).
pub fn resolveLeaseScope(hx: Hx, conn: *pg.Conn, runner_id: []const u8, lease_id: []const u8) !?LeaseScope {
    var q = PgQuery.from(try conn.query(sql.SELECT_LEASE_SCOPE_FOR_MINT, .{ lease_id, runner_id, protocol.RUNNER_LEASE_STATUS_ACTIVE, constants.clock.nowMillis() }));
    defer q.deinit();
    const row = try q.next() orelse return null;
    const workspace_id = try hx.alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer hx.alloc.free(workspace_id);
    const fleet_id = try hx.alloc.dupe(u8, try row.get([]const u8, 1));
    errdefer hx.alloc.free(fleet_id);
    const binding = repositoryBinding(hx, try row.get([]const u8, 2));
    return .{ .workspace_id = workspace_id, .fleet_id = fleet_id, .repository_binding = binding };
}

/// Extract the fleet's repository EGRESS binding from its `config_json`.
///
/// A parse failure degrades to null, which the repository-scoped mint treats as
/// "no binding" and REFUSES — so a malformed config withholds a token rather
/// than widening one. Everything allocated here is arena-owned (`hx.alloc`) and
/// released with the request, so `FleetConfig.deinit` is deliberately not called:
/// the row-backed slices it would free belong to the arena, not to us.
fn repositoryBinding(hx: Hx, config_json: []const u8) ?integration.RepositoryBinding {
    const cfg = fleet_config.parseFleetConfig(hx.alloc, config_json) catch |err| {
        log.warn("credential_mint_config_unparsed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
        return null;
    };
    return cfg.repository_binding;
}
