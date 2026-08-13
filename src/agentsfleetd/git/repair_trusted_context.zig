//! Daemon-authored repair context for an approved write-bound lease.

const std = @import("std");
const logging = @import("log");

const hx_mod = @import("../http/handlers/hx.zig");
const ec = @import("../errors/error_registry.zig");
const repair_branch = @import("repair_branch.zig");
const repository_http_policy = @import("repository_http_policy.zig");
const assign = @import("../fleet/assign.zig");
const FleetSession = @import("../fleet/fleet_session.zig");
const approval_gate_db = @import("../fleet_runtime/approval_gate_db.zig");
const execution_policy = @import("contract").execution_policy;

const Hx = hx_mod.Hx;
const log = logging.scoped(.runner_lease);

pub const Resolved = struct {
    instructions: []const u8,
    http_origin_policies: []const execution_policy.HttpOriginPolicy = &.{},
};

/// Write-bound runs receive the exact approved-gate branch as daemon-authored
/// instructions. Every other Fleet receives its installed instructions intact.
pub fn resolve(hx: Hx, session: *FleetSession, acq: assign.Acquired) ?Resolved {
    const binding = session.config.repository_binding orelse return .{ .instructions = session.instructions };
    if (binding.access != .write) return .{
        .instructions = session.instructions,
        .http_origin_policies = repository_http_policy.build(hx.alloc, binding, null) catch return null,
    };
    if (binding.repositories.len != 1) {
        log.warn("repair_branch_repository_binding_refused", .{ .error_code = ec.ERR_REPAIR_WRITE_UNAPPROVED, .fleet_id = acq.fleet_id, .event_id = acq.event_id, .repository_count = binding.repositories.len });
        return null;
    }
    const gate_id = approval_gate_db.approvedWriteGateId(hx.ctx.pool, hx.alloc, acq.fleet_id, acq.event_id, binding) catch |err| {
        log.warn("repair_branch_gate_lookup_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .fleet_id = acq.fleet_id, .event_id = acq.event_id, .err = @errorName(err) });
        return null;
    } orelse {
        log.warn("repair_branch_gate_missing", .{ .error_code = ec.ERR_REPAIR_WRITE_UNAPPROVED, .fleet_id = acq.fleet_id, .event_id = acq.event_id });
        return null;
    };
    defer hx.alloc.free(gate_id);
    const branch = repair_branch.fromGateId(gate_id) catch return null;
    const branch_copy = hx.alloc.dupe(u8, &branch) catch return null;
    const base = binding.base_branch orelse return null;
    const instructions = std.fmt.allocPrint(
        hx.alloc,
        "{s}\n\n## Trusted repair context\n\nRepository: `{s}`. Trusted base: `{s}`. Repair branch: `{s}`. Copy all three exactly for GitHub reads and writes; never construct or alter them.",
        .{ session.instructions, binding.repositories[0], base, branch },
    ) catch {
        hx.alloc.free(branch_copy);
        return null;
    };
    return .{
        .instructions = instructions,
        .http_origin_policies = repository_http_policy.build(hx.alloc, binding, branch_copy) catch return null,
    };
}
