//! Install-time origination for integration grants.
//!
//! `core.integration_grants` is the enforcement spine for credential minting —
//! `credentials_mint`, the lease classifier and the App ingress routing query
//! all gate on an approved row. Until this module existed, the only production
//! statement that could CREATE such a row sat behind the external fleet-key
//! route, so an internally-installed fleet declaring a mintable credential
//! could never obtain one: ingress excluded it, no event was written, no lease
//! issued, and nothing reported that a decision was owed.
//!
//! Installing is where the requirement becomes knowable, and it is the last
//! moment before any traffic. So install seeds the `pending` grant and raises
//! the approval gate that asks for it. The gate is the asking mechanism; the
//! grant is the memory of the answer, which is why both survive and neither is
//! redundant — and why resolving the gate moves the grant in one statement
//! (`fleet_runtime/sql.zig` RESOLVE_GATE).
//!
//! Which credentials count is NOT re-derived here. `secrets_resolve.mintableId`
//! is the same classifier the lease path uses to decide what may mint, so the
//! ask and the enforcement cannot disagree about what an integration is.

const std = @import("std");
const logging = @import("log");
const clock = @import("common").clock;

const sql = @import("sql.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const secrets_resolve = @import("../../../fleet/secrets_resolve.zig");
const integration = @import("../../../credentials/integration.zig");
const grant_lookup = @import("../../../state/integration_grant_lookup.zig");
const approval_gate = @import("../../../fleet_runtime/approval_gate.zig");
const approval_gate_db = @import("../../../fleet_runtime/approval_gate_db.zig");
const gate_constants = @import("../../../fleet_runtime/approval_gate_constants.zig");

const log = logging.scoped(.install_grants);

const PENDING = grant_lookup.GrantStatus.pending.toSlice();
const S_TOOL = "integration";
const S_ACTION = "grant";
const S_BLAST_RADIUS = "workspace";
/// The grant's `requested_reason` — why this row exists at all. The bundle
/// carries no free-text justification field today, so origination is the whole
/// reason and it is stated once here rather than at the INSERT. `pub` because
/// the origination test asserts the stored reason against it (RULE UFS: the
/// operator reads this in the inbox, so a test must not spell it a second time).
pub const S_DEFAULT_REASON = "Declared by the fleet bundle at install";

/// Seed a pending grant and raise its approval gate for every mintable
/// credential the fleet declares. Best-effort per credential and never fatal to
/// the install: a fleet that installs without its gate is visibly un-armed (the
/// grant stays absent, ingress excludes it) rather than silently authorized.
/// The inverse — failing the install — would strand a created fleet row.
pub fn seedForInstall(
    hx: hx_mod.Hx,
    workspace_id: []const u8,
    fleet_id: []const u8,
    credentials: []const []const u8,
) void {
    if (credentials.len == 0) return;

    const resolved = secrets_resolve.resolveSecretsMap(hx.alloc, hx.ctx.pool, workspace_id, credentials) catch |err| {
        // Missing credentials are already refused upstream by
        // `create_fleet_bundle.ensureBundleCredentials`, so reaching here means
        // a transient read, not a bad request.
        log.warn("install_grant_resolve_failed", .{
            .error_code = ec.ERR_INTERNAL_DB_QUERY,
            .err = @errorName(err),
            .fleet_id = fleet_id,
        });
        return;
    };
    defer secrets_resolve.freeResolved(hx.alloc, resolved);

    for (resolved) |entry| {
        const id = secrets_resolve.mintableId(entry.parsed.value) orelse continue;
        seedOne(hx, workspace_id, fleet_id, integration.toString(id), entry.name);
    }
}

fn seedOne(
    hx: hx_mod.Hx,
    workspace_id: []const u8,
    fleet_id: []const u8,
    service: []const u8,
    credential_name: []const u8,
) void {
    const now_ms = clock.nowMillis();

    const grant_uid = id_format.generateUuidV7() catch return;
    const grant_id: []const u8 = &grant_uid;

    const conn = hx.ctx.pool.acquire() catch |err| {
        log.warn("install_grant_acquire_failed", .{
            .error_code = ec.ERR_INTERNAL_DB_QUERY,
            .err = @errorName(err),
            .fleet_id = fleet_id,
        });
        return;
    };
    defer hx.ctx.pool.release(conn);

    _ = conn.exec(sql.INSERT_PENDING_GRANT, .{
        grant_id, fleet_id, service, PENDING, S_DEFAULT_REASON, now_ms,
    }) catch |err| {
        log.warn("install_grant_insert_failed", .{
            .error_code = ec.ERR_INTERNAL_DB_QUERY,
            .err = @errorName(err),
            .fleet_id = fleet_id,
            .integration = service,
        });
        return;
    };

    raiseGate(hx, workspace_id, fleet_id, service, credential_name);
}

/// Raise the inbox entry that asks a human to answer for `service`.
///
/// `evidence` carries the service under the key RESOLVE_GATE reads, so the
/// approval can move the grant without a second lookup. `action_id` is derived
/// from (fleet, service) rather than minted fresh: re-installing the same fleet
/// must not stack duplicate questions in the inbox.
fn raiseGate(
    hx: hx_mod.Hx,
    workspace_id: []const u8,
    fleet_id: []const u8,
    service: []const u8,
    credential_name: []const u8,
) void {
    const action_id = std.fmt.allocPrint(hx.alloc, "grant:{s}:{s}", .{ fleet_id, service }) catch return;
    defer hx.alloc.free(action_id);

    const evidence = std.fmt.allocPrint(
        hx.alloc,
        "{{\"{s}\":\"{s}\",\"credential\":\"{s}\"}}",
        .{ gate_constants.GATE_EVIDENCE_SERVICE_KEY, service, credential_name },
    ) catch return;
    defer hx.alloc.free(evidence);

    const proposed = std.fmt.allocPrint(
        hx.alloc,
        "Use {s} on behalf of this fleet",
        .{service},
    ) catch return;
    defer hx.alloc.free(proposed);

    approval_gate_db.recordGatePending(hx.ctx.pool, hx.alloc, fleet_id, workspace_id, action_id, approval_gate.ActionDetail{
        .tool = S_TOOL,
        .action = S_ACTION,
        .params_summary = service,
        .gate_kind = gate_constants.GATE_KIND_INTEGRATION_GRANT,
        .proposed_action = proposed,
        .evidence_json = evidence,
        .blast_radius = S_BLAST_RADIUS,
    });
}
