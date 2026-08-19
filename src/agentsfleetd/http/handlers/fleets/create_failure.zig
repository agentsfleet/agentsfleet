//! The two failure responses of the fleet create path that no test can trigger
//! through the handler.
//!
//! `innerCreateFleet` writes both from inside a `catch` whose cause cannot be
//! staged on demand. The name generator only fails when the request arena
//! cannot allocate; an insert only fails for a non-unique reason on a database
//! fault this schema will not produce — the INSERT selects from
//! `core.workspaces`, so a missing workspace writes zero rows and raises
//! nothing, and the one deterministic alternative (a trigger that raises) is
//! global to `core.fleets` and would break the seven unit lanes that insert
//! concurrently against one database.
//!
//! Emitted from here, each response is asserted against a real `httpz.Response`
//! rather than trusted to read correctly — the same split `hx_test.zig` uses to
//! prove `hx.ok` and `hx.fail`, and the same reasoning as
//! `hx.classifyAcquireError`: when the trigger is unstageable, the emitted
//! behaviour is still the whole content of the decision.

const logging = @import("log");
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");

const log = logging.scoped(.fleet_api);

/// 500 for a fleet name the server could not generate. Distinct from the DB
/// error below: nothing is wrong with the datastore, so the operator-facing
/// detail names the operation rather than the dependency.
pub fn nameGenerationFailed(hx: hx_mod.Hx) void {
    common.internalOperationError(hx.res, "name generation failed", hx.req_id);
}

/// 500 for an insert that failed for any reason other than the name being
/// taken. Logs the underlying error name — the response deliberately does not
/// carry it, so the log line is the only place the cause survives.
pub fn insertFailed(hx: hx_mod.Hx, err: anyerror) void {
    log.err("create_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err), .req_id = hx.req_id });
    common.internalDbError(hx.res, hx.req_id);
}
