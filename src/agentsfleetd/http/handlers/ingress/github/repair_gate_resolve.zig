//! GitHub repair ownership resolution through durable approval state.

const std = @import("std");
const pg = @import("pg");

const PgQuery = @import("../../../../db/pg_query.zig").PgQuery;
const repair_branch = @import("../../../../git/repair_branch.zig");
const approval_gate = @import("../../../../fleet_runtime/approval_gate.zig");
const gate_constants = @import("../../../../fleet_runtime/approval_gate_constants.zig");
const config_types = @import("../../../../fleet_runtime/config_types.zig");
const binding_json = @import("../../../../fleet_runtime/repository_binding_json.zig");
const github_spec = @import("../../connectors/github/spec.zig");
const sql = @import("../../../../state/repair_sql.zig");

pub const Owner = struct {
    const Self = @This();

    workspace_id: []u8,
    fleet_id: []u8,
    event_id: []u8,
    base_branch: []u8,

    pub fn deinit(self: *Self, alloc: std.mem.Allocator) void {
        alloc.free(self.workspace_id);
        alloc.free(self.fleet_id);
        alloc.free(self.event_id);
        alloc.free(self.base_branch);
    }
};

pub const Result = union(enum) {
    owner: Owner,
    invalid_reference,
    refused,
};

pub const Authority = union(enum) {
    fleet: []const u8,
    installation: []const u8,
};

/// Resolve one branch to the gate's exact workspace, Fleet, and incident event.
pub fn resolve(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    branch: []const u8,
    workspace_id: []const u8,
    authority: Authority,
    repository: []const u8,
) !Result {
    const gate_id = repair_branch.gateId(branch) catch return .invalid_reference;
    const fleet_id_filter = switch (authority) {
        .fleet => |id| id,
        .installation => "",
    };
    const installation_id = switch (authority) {
        .fleet => "",
        .installation => |id| id,
    };
    var q = PgQuery.from(try conn.query(sql.RESOLVE_REPAIR_GATE_OWNER, .{
        &gate_id,
        workspace_id,
        fleet_id_filter,
        approval_gate.GateStatus.approved.toSlice(),
        gate_constants.GATE_KIND_REPOSITORY_WRITE,
        github_spec.PROVIDER,
        installation_id,
        binding_json.FIELD_ACCESS,
        config_types.S_REPOSITORY_ACCESS_WRITE,
        binding_json.FIELD_REPOSITORIES,
        repository,
        gate_constants.REPOSITORY_WRITE_SPEND_CEILING,
        binding_json.FIELD_BASE,
    }));
    defer q.deinit();
    const row = try q.next() orelse return .refused;
    return .{ .owner = try readOwner(alloc, row) };
}

fn readOwner(alloc: std.mem.Allocator, row: pg.Row) !Owner {
    const workspace_id = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(workspace_id);
    const fleet_id = try alloc.dupe(u8, try row.get([]const u8, 1));
    errdefer alloc.free(fleet_id);
    const event_id = try alloc.dupe(u8, try row.get([]const u8, 2));
    errdefer alloc.free(event_id);
    const base_branch = try alloc.dupe(u8, try row.get([]const u8, 3));
    return .{
        .workspace_id = workspace_id,
        .fleet_id = fleet_id,
        .event_id = event_id,
        .base_branch = base_branch,
    };
}
